// Tests for decode.js: `node --test decoder/`. No npm packages.

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const CODE = fs.readFileSync(path.join(__dirname, "decode.js"), "utf8");
const SCHEMAS = JSON.parse(fs.readFileSync(path.join(__dirname, "..", "schemas.json"), "utf8"));

// A schema with every type: kind 200 "test", next to the real battery.
const TEST_SCHEMAS = {
    kinds: {
        4: SCHEMAS.kinds[4],
        200: {
            source: "test",
            fields: [
                { key: "a", type: "u8", divisor: 1 },
                { key: "b", type: "i8", divisor: 1 },
                { key: "c", type: "u16", divisor: 10 },
                { key: "d", type: "i16", divisor: 100 },
                { key: "e", type: "u32", divisor: 1 },
                { key: "f", type: "i32", divisor: 1000 },
            ],
        },
    },
    nodes: {
        100: { name: "both", heartbeat: 60, sources: ["battery", "test"] },
        101: { name: "battery-only", heartbeat: 300, sources: ["battery"] },
    },
    devices: {},
    reserved_node_ids: [0, 124],
};

// The body of a kind 200 section, as hex.
function testBody(a, b, c, d, e, f) {
    const body = Buffer.alloc(14);
    body.writeUInt8(a, 0);
    body.writeInt8(b, 1);
    body.writeUInt16LE(c, 2);
    body.writeInt16LE(d, 4);
    body.writeUInt32LE(e, 6);
    body.writeInt32LE(f, 10);
    return body.toString("hex");
}

// A fresh decode.js, run the way Node-RED runs a function node, with its own
// flow context. `send` hands it one message and returns what it outputs.
function decoder(schemas) {
    const run = new Function("msg", "node", "flow", CODE);
    const context = new Map();
    const warnings = [];
    const node = { warn: (text) => warnings.push(String(text)) };
    const flow = { get: (key) => context.get(key), set: (key, value) => context.set(key, value) };
    const send = (msg) => run(msg, node, flow);
    if (schemas) {
        send({ topic: "aio/schemas", payload: schemas });
    }
    return { send, warnings };
}

// A packet as OpenMQTTGateway publishes it.
function packet(hex, rssi = -71, snr = 9.5) {
    return { topic: "omg/openmqttgateway/LORAtoMQTT", payload: { rssi, snr, hex } };
}

// The state message among the messages on output 2.
function stateOf(output) {
    assert.ok(output, "no output");
    assert.equal(output[0], null);
    const states = output[1].filter((msg) => msg.topic.endsWith("/state"));
    assert.equal(states.length, 1);
    return states[0];
}

test("the message from dev-c6, with the real schemas.json", () => {
    const { send, warnings } = decoder(SCHEMAS);
    const msg = {
        topic: "omg/openmqttgateway/LORAtoMQTT",
        payload: { rssi: -94, snr: 11.25, pferror: 9739, packetSize: 3, hex: "640440" },
    };
    assert.deepEqual(stateOf(send(msg)), {
        topic: "aio/100/state",
        payload: { battery: { voltage: 1.28 }, rssi: -94, snr: 11.25 },
        retain: false,
    });
    assert.deepEqual(warnings, []);
});

test("battery 0xb9 is 3.7 V", () => {
    const { send } = decoder(SCHEMAS);
    assert.deepEqual(stateOf(send(packet("6404b9"))).payload, {
        battery: { voltage: 3.7 },
        rssi: -71,
        snr: 9.5,
    });
});

test("battery 0xff is null", () => {
    const { send } = decoder(SCHEMAS);
    assert.deepEqual(stateOf(send(packet("6404ff"))).payload.battery, { voltage: null });
});

test("every type, with negative values", () => {
    const { send, warnings } = decoder(TEST_SCHEMAS);
    const body = testBody(200, -5, 1234, -1234, 4000000000, -123456);
    const state = stateOf(send(packet(`6404b9c8${body}`))).payload;
    assert.deepEqual(state.test, { a: 200, b: -5, c: 123.4, d: -12.34, e: 4000000000, f: -123.456 });
    assert.deepEqual(warnings, []);
});

test("little-endian", () => {
    const { send } = decoder(TEST_SCHEMAS);
    // c = 0x0102 = 258, d = 0x0304 = 772, e = 0x05060708, f = 0x090a0b0c.
    const body = "0000" + "0201" + "0403" + "08070605" + "0c0b0a09";
    const state = stateOf(send(packet(`6404b9c8${body}`))).payload;
    assert.equal(state.test.c, 25.8);
    assert.equal(state.test.d, 7.72);
    assert.equal(state.test.e, 0x05060708);
    assert.equal(state.test.f, 0x090a0b0c / 1000);
});

test("missing values are null, their neighbours aren't", () => {
    const { send } = decoder(TEST_SCHEMAS);
    const missing = testBody(0xff, -0x80, 0xffff, -0x8000, 0xffffffff, -0x80000000);
    assert.deepEqual(stateOf(send(packet(`6404b9c8${missing}`))).payload.test, {
        a: null, b: null, c: null, d: null, e: null, f: null,
    });

    const edges = testBody(0xfe, -0x7f, 0xfffe, -0x7fff, 0xfffffffe, -0x7fffffff);
    assert.deepEqual(stateOf(send(packet(`6404b9c8${edges}`))).payload.test, {
        a: 254, b: -127, c: 6553.4, d: -327.67, e: 4294967294, f: -2147483.647,
    });
});

test("unknown kind is dropped", () => {
    const { send, warnings } = decoder(SCHEMAS);
    assert.equal(send(packet("640900")), null);
    assert.deepEqual(warnings, ["packet 640900 from node 100 dropped: unknown kind 9 at byte 1"]);
});

test("too short is dropped", () => {
    const { send, warnings } = decoder(SCHEMAS);
    assert.equal(send(packet("6404")), null);
    assert.equal(send(packet("64")), null);
    assert.deepEqual(warnings, [
        "packet 6404 from node 100 dropped: kind 4 needs 1 bytes, but only 0 are left",
        "packet 64 from node 100 dropped: no sections",
    ]);
});

test("too long is dropped", () => {
    const { send, warnings } = decoder(SCHEMAS);
    assert.equal(send(packet("6404b900")), null);
    assert.deepEqual(warnings, ["packet 6404b900 from node 100 dropped: unknown kind 0 at byte 3"]);
});

test("a kind twice is dropped", () => {
    const { send, warnings } = decoder(SCHEMAS);
    assert.equal(send(packet("6404b904b9")), null);
    assert.deepEqual(warnings, ["packet 6404b904b9 from node 100 dropped: kind 4 twice"]);
});

test("a reserved node ID goes to output 1, unchanged", () => {
    const { send, warnings } = decoder(SCHEMAS);
    const legacy = packet("7cdfa1f901a000000102");
    const output = send(legacy);
    assert.equal(output[0], legacy);
    assert.equal(output[1], null);
    assert.deepEqual(legacy, packet("7cdfa1f901a000000102"));
    assert.deepEqual(warnings, []);
});

test("without schemas.json, packets are dropped", () => {
    const { send, warnings } = decoder();
    assert.equal(send(packet("6404b9")), null);
    assert.equal(warnings.length, 1);
    assert.match(warnings[0], /no schemas\.json yet/);
});

test("a source missing from the packet is published as null", () => {
    const { send, warnings } = decoder(TEST_SCHEMAS);
    const state = stateOf(send(packet("6404b9"))).payload;
    assert.deepEqual(state.battery, { voltage: 3.7 });
    assert.deepEqual(state.test, { a: null, b: null, c: null, d: null, e: null, f: null });
    assert.deepEqual(warnings, ["node both (100) sent [battery], but nodes.toml gives it [battery, test]"]);
});

test("a source the node doesn't have is still published", () => {
    const { send, warnings } = decoder(TEST_SCHEMAS);
    const body = testBody(1, 2, 3, 4, 5, 6);
    const state = stateOf(send(packet(`65c8${body}04b9`))).payload;
    assert.deepEqual(state.battery, { voltage: 3.7 });
    assert.equal(state.test.a, 1);
    assert.deepEqual(warnings, [
        "node battery-only (101) sent [test, battery], but nodes.toml gives it [battery]",
    ]);
});

test("a node that isn't in schemas.json is still decoded", () => {
    const { send } = decoder(SCHEMAS);
    assert.deepEqual(stateOf(send(packet("6904b9"))), {
        topic: "aio/105/state",
        payload: { battery: { voltage: 3.7 }, rssi: -71, snr: 9.5 },
        retain: false,
    });
});

test("a message without a packet is dropped", () => {
    const { send, warnings } = decoder(SCHEMAS);
    assert.equal(send({ topic: "omg/openmqttgateway/LORAtoMQTT", payload: '{"hex":"6404b9"}' }), null);
    assert.equal(send(packet("6404b")), null);
    assert.equal(warnings.length, 2);
});

test("a broken schemas.json is ignored", () => {
    const { send, warnings } = decoder(SCHEMAS);
    assert.equal(send({ topic: "aio/schemas", payload: "" }), null);
    assert.equal(warnings.length, 1);
    // The old one still decodes.
    assert.equal(stateOf(send(packet("6404b9"))).payload.battery.voltage, 3.7);
});
