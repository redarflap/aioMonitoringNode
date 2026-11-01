// aio decoder: turns LoRa packets into Home Assistant state, driven by
// schemas.json, with no per-kind code. This file is the body of one Node-RED
// function node with 2 outputs:
//   1: legacy packets (first byte in reserved_node_ids), unchanged
//   2: MQTT messages with topic, payload and retain, for an MQTT out node
// Input: schemas.json on `aio/schemas`, and OpenMQTTGateway's packets, for
// example {"rssi": -94, "snr": 11.25, "hex": "640440", ...}.

// Flow context key of the latest schemas.json.
const SCHEMAS = "aio_schemas";

// The wire types: size in bytes, how to read one (little-endian), and the
// missing value (MAX unsigned, MIN signed).
const TYPES = {
    u8: { size: 1, read: (view, i) => view.getUint8(i), missing: 0xff },
    i8: { size: 1, read: (view, i) => view.getInt8(i), missing: -0x80 },
    u16: { size: 2, read: (view, i) => view.getUint16(i, true), missing: 0xffff },
    i16: { size: 2, read: (view, i) => view.getInt16(i, true), missing: -0x8000 },
    u32: { size: 4, read: (view, i) => view.getUint32(i, true), missing: 0xffffffff },
    i32: { size: 4, read: (view, i) => view.getInt32(i, true), missing: -0x80000000 },
};

if (msg.topic === "aio/schemas") {
    return keepSchemas(msg.payload);
}
return decodePacket(msg);

// Keeps a new schemas.json in flow context.
function keepSchemas(schemas) {
    const valid =
        schemas && typeof schemas === "object" && schemas.kinds && schemas.nodes && schemas.devices
        && Array.isArray(schemas.reserved_node_ids);
    if (!valid) {
        node.warn("aio/schemas holds no schemas.json; is the MQTT in node set to auto-detect?");
        return null;
    }
    flow.set(SCHEMAS, schemas);
    return null;
}

// One packet from OpenMQTTGateway: a legacy packet goes to output 1 as it
// is, every other packet becomes the state JSON of its node.
function decodePacket(msg) {
    const schemas = flow.get(SCHEMAS);
    if (!schemas) {
        node.warn("no schemas.json yet, so the packet is dropped; run `cargo xtask schemas --publish`");
        return null;
    }
    const hex = msg.payload && msg.payload.hex;
    if (typeof hex !== "string" || !/^([0-9a-f]{2})+$/i.test(hex)) {
        node.warn(`no packet in the message on ${msg.topic}; is the MQTT in node set to auto-detect?`);
        return null;
    }
    const bytes = Uint8Array.from(hex.match(/../g), (byte) => parseInt(byte, 16));
    const nodeId = bytes[0];
    if (schemas.reserved_node_ids.includes(nodeId)) {
        return [msg, null];
    }

    let sections;
    try {
        sections = readSections(schemas, bytes);
    } catch (e) {
        node.warn(`packet ${hex} from node ${nodeId} dropped: ${e.message}`);
        return null;
    }

    const known = schemas.nodes[nodeId];
    if (known) {
        const sent = Object.keys(sections);
        const same = sent.length === known.sources.length && sent.every((s) => known.sources.includes(s));
        if (!same) {
            node.warn(
                `node ${known.name} (${nodeId}) sent [${sent.join(", ")}], `
                + `but nodes.toml gives it [${known.sources.join(", ")}]`,
            );
        }
        // A missing source's fields become null, so HA shows them as
        // unknown instead of keeping an old value.
        for (const source of known.sources) {
            const kind = kindOf(schemas, source);
            if (kind && !(source in sections)) {
                sections[source] = Object.fromEntries(kind.fields.map((field) => [field.key, null]));
            }
        }
    }

    const state = { ...sections, rssi: msg.payload.rssi ?? null, snr: msg.payload.snr ?? null };
    return [null, [{ topic: `aio/${nodeId}/state`, payload: state, retain: false }]];
}

// Reads the sections `{ [kind] [body] }+` after the node ID. Returns
// source -> { field key -> value }; a value is raw / divisor, or null when
// missing. Throws if the packet doesn't match the schemas.
function readSections(schemas, bytes) {
    if (bytes.length < 2) {
        throw new Error("no sections");
    }
    const view = new DataView(bytes.buffer);
    const sections = {};
    let i = 1;
    while (i < bytes.length) {
        const number = bytes[i];
        const kind = schemas.kinds[number];
        if (!kind) {
            throw new Error(`unknown kind ${number} at byte ${i}`);
        }
        if (kind.source in sections) {
            throw new Error(`kind ${number} twice`);
        }
        i += 1;
        const size = kind.fields.reduce((sum, field) => sum + typeOf(field).size, 0);
        if (i + size > bytes.length) {
            throw new Error(`kind ${number} needs ${size} bytes, but only ${bytes.length - i} are left`);
        }
        const values = {};
        for (const field of kind.fields) {
            const type = typeOf(field);
            const raw = type.read(view, i);
            values[field.key] = raw === type.missing ? null : raw / field.divisor;
            i += type.size;
        }
        sections[kind.source] = values;
    }
    return sections;
}

function typeOf(field) {
    const type = TYPES[field.type];
    if (!type) {
        throw new Error(`decode.js doesn't know type ${field.type}; paste the new decode.js into Node-RED`);
    }
    return type;
}

// The kind of a source, or undefined while the source isn't ported yet.
function kindOf(schemas, source) {
    return Object.values(schemas.kinds).find((kind) => kind.source === source);
}
