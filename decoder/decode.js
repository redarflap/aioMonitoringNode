// aio decoder: turns LoRa packets into Home Assistant sensors, driven by
// schemas.json, with no per-kind code. ARCHITECTURE.md, "Decoding & Home
// Assistant", has the rules.
//
// Set it up once in Node-RED:
// 1. A function node with 2 outputs, and this whole file pasted in as its
//    body (the "On Message" tab).
// 2. Three MQTT in nodes, each with output "auto-detect", into the function
//    node: `aio/schemas`, `homeassistant/status` and
//    `omg/openmqttgateway/LORAtoMQTT`.
// 3. Output 1 (legacy packets, unchanged) into the legacy flow, in place of
//    its own MQTT in node.
// 4. Output 2 into an MQTT out node with Topic, QoS and Retain left empty,
//    so they come from each message.
// 5. Deploy, then `cargo xtask schemas --publish`.
// Paste this file again only when it changes; changes to nodes.toml arrive
// over `aio/schemas`.

// Flow context keys: the latest schemas.json, and node ID -> sources of
// the nodes that send but aren't in schemas.json.
const SCHEMAS = "aio_schemas";
const UNKNOWN = "aio_unknown_nodes";

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
if (msg.topic === "homeassistant/status") {
    // HA has restarted: it needs the discovery messages again.
    const schemas = flow.get(SCHEMAS);
    return msg.payload === "online" && schemas ? [null, discovery(schemas)] : null;
}
return decodePacket(msg);

// Keeps a new schemas.json in flow context and publishes discovery from it.
// The retained one also arrives when Node-RED starts.
function keepSchemas(schemas) {
    const valid =
        schemas && typeof schemas === "object" && schemas.kinds && schemas.nodes && schemas.devices
        && Array.isArray(schemas.reserved_node_ids);
    if (!valid) {
        node.warn("aio/schemas holds no schemas.json; is the MQTT in node set to auto-detect?");
        return null;
    }
    flow.set(SCHEMAS, schemas);
    return [null, discovery(schemas)];
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

    const out = [];
    const known = schemas.nodes[nodeId];
    if (!known) {
        // Not in nodes.toml: on its first packet, a default device with the
        // sources it sent.
        const unknown = flow.get(UNKNOWN) || {};
        if (!(nodeId in unknown)) {
            unknown[nodeId] = Object.keys(sections);
            flow.set(UNKNOWN, unknown);
            out.push(unknownDevice(schemas, nodeId, unknown[nodeId]));
        }
    } else {
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
    out.push({ topic: `aio/${nodeId}/state`, payload: state, retain: false });
    return [null, out];
}

// One retained discovery message per HA device: each node's default device
// with its sources that no declared device takes, plus RSSI and SNR; each
// declared device; and each unknown node seen so far.
function discovery(schemas) {
    // `<node_id>.<source>` of every source in a declared device.
    const declared = new Set();
    for (const device of Object.values(schemas.devices)) {
        for (const { node: nodeId, source } of device.sources) {
            declared.add(`${nodeId}.${source}`);
        }
    }

    const messages = [];
    for (const [nodeId, known] of Object.entries(schemas.nodes)) {
        const components = {};
        for (const source of known.sources) {
            if (!declared.has(`${nodeId}.${source}`)) {
                Object.assign(components, sourceComponents(schemas, nodeId, source, known.heartbeat));
            }
        }
        Object.assign(components, signalComponents(nodeId, known.heartbeat));
        messages.push(deviceConfig(`aio_${nodeId}`, known.name, components));
    }
    for (const [key, device] of Object.entries(schemas.devices)) {
        const components = {};
        for (const { node: nodeId, source } of device.sources) {
            const heartbeat = schemas.nodes[nodeId].heartbeat;
            Object.assign(components, sourceComponents(schemas, nodeId, source, heartbeat));
        }
        messages.push(deviceConfig(`aio_${key}`, device.name, components));
    }
    for (const [nodeId, sources] of Object.entries(flow.get(UNKNOWN) || {})) {
        if (!(nodeId in schemas.nodes)) {
            messages.push(unknownDevice(schemas, nodeId, sources));
        }
    }
    return messages;
}

// The default device of a node that isn't in schemas.json. Without a
// heartbeat, its sensors never expire.
function unknownDevice(schemas, nodeId, sources) {
    const components = {};
    for (const source of sources) {
        Object.assign(components, sourceComponents(schemas, nodeId, source, null));
    }
    Object.assign(components, signalComponents(nodeId, null));
    return deviceConfig(`aio_${nodeId}`, `aio ${nodeId}`, components);
}

// The discovery message of one device. `id` is `aio_<node_id>` or `aio_<key>`.
function deviceConfig(id, name, components) {
    return {
        topic: `homeassistant/device/${id}/config`,
        payload: {
            device: { identifiers: [id], name },
            origin: { name: "aio decoder" },
            components,
        },
        retain: true,
    };
}

// One sensor per field of a node's source. None while the source isn't
// ported yet.
function sourceComponents(schemas, nodeId, source, heartbeat) {
    const components = {};
    const kind = kindOf(schemas, source);
    for (const field of kind ? kind.fields : []) {
        const id = `aio_${nodeId}_${source}_${field.key}`;
        components[id] = sensor(id, `${source} ${field.key}`, nodeId, `${source}.${field.key}`, heartbeat, {
            unit_of_measurement: field.unit,
            device_class: field.device_class,
            state_class: field.state_class,
        });
    }
    return components;
}

// RSSI and SNR of a node's packets, from OpenMQTTGateway, as diagnostics.
function signalComponents(nodeId, heartbeat) {
    const rssi = `aio_${nodeId}_rssi`;
    const snr = `aio_${nodeId}_snr`;
    return {
        [rssi]: sensor(rssi, "RSSI", nodeId, "rssi", heartbeat, {
            unit_of_measurement: "dBm",
            device_class: "signal_strength",
            state_class: "measurement",
            entity_category: "diagnostic",
        }),
        [snr]: sensor(snr, "SNR", nodeId, "snr", heartbeat, {
            unit_of_measurement: "dB",
            state_class: "measurement",
            entity_category: "diagnostic",
        }),
    };
}

// One sensor component. `path` is where its value sits in the node's state
// JSON, for example `battery.voltage`. Options without a value are left
// out. HA marks the sensor unavailable after 2.5 heartbeats without a packet.
function sensor(uniqueId, name, nodeId, path, heartbeat, options) {
    const component = {
        platform: "sensor",
        unique_id: uniqueId,
        name,
        state_topic: `aio/${nodeId}/state`,
        value_template: `{{ value_json.${path} }}`,
    };
    for (const [key, value] of Object.entries(options)) {
        if (value != null) {
            component[key] = value;
        }
    }
    if (heartbeat) {
        component.expire_after = Math.ceil(2.5 * heartbeat);
    }
    return component;
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
