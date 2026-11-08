import { type ChildProcess, spawn } from "node:child_process";
import { createWriteStream, existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";

const uiWebDir = path.resolve(import.meta.dirname, "..");
const daemonBin = process.env.AXUS_DAEMON_BIN ?? path.resolve(uiWebDir, "../daemon/target/debug/omnius-axus-daemon");

export const LIBRARY_FILE_NAME = "hello.txt";
export const LIBRARY_FILE_TEXT = "hello from the library\n";

type Node = { child: ChildProcess; api: number; output: () => string };

async function freePort() {
    return await new Promise<number>((resolve, reject) => {
        const server = net.createServer();
        server.once("error", reject);
        server.listen(0, "127.0.0.1", () => {
            const { port } = server.address() as net.AddressInfo;
            server.close(() => resolve(port));
        });
    });
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
// biome-ignore lint/suspicious/noControlCharactersInRegex: strips ANSI escapes from the daemon's log.
const stripAnsi = (text: string) => text.replace(/\x1b\[[0-9;]*m/g, "");

// Writes the configuration of one daemon under `root/<name>` and starts it. The daemon serves the build in `dist`.
async function startNode(
    root: string,
    name: string,
    extra: { bootstrap?: string; libraryRoot?: string },
    started: Node[],
): Promise<Node> {
    const dir = path.join(root, name);
    const configDir = path.join(dir, "config");
    mkdirSync(configDir, { recursive: true });
    const api = await freePort();
    const p2p = await freePort();
    const lines = [
        "[core]",
        `state_dir = ${JSON.stringify(path.join(dir, "state"))}`,
        "[api]",
        `listen_addr = "127.0.0.1:${api}"`,
        "[p2p]",
        `listen_addr = "127.0.0.1:${p2p}"`,
        "advertise_addrs = []",
        "use_upnp = false",
        `bootstrap_nodes = [${extra.bootstrap ? JSON.stringify(extra.bootstrap) : ""}]`,
        "[logging]",
        'level = "info"',
        "json = false",
        "[storage]",
        `library_roots = [${extra.libraryRoot ? JSON.stringify(extra.libraryRoot) : ""}]`,
        "[web]",
        `dir = ${JSON.stringify(path.join(uiWebDir, "dist"))}`,
        // The tests reach the daemon over plain HTTP on loopback.
        "insecure_cookie = true",
    ];
    writeFileSync(path.join(configDir, "axus.toml"), `${lines.join("\n")}\n`);

    const child = spawn(daemonBin, ["start", "--config-dir", configDir], {
        cwd: dir,
        stdio: ["ignore", "pipe", "pipe"],
    });
    const log = createWriteStream(path.join(dir, "daemon.log"));
    let output = "";
    for (const stream of [child.stdout, child.stderr]) {
        stream.on("data", (chunk: Buffer) => {
            output += stripAnsi(chunk.toString());
            log.write(chunk);
        });
    }
    const node: Node = { child, api, output: () => output };
    // Registered before the readiness wait, so a daemon that never answers is still stopped.
    started.push(node);

    const deadline = Date.now() + 60_000;
    while (Date.now() < deadline) {
        if (child.exitCode !== null) throw new Error(`the daemon "${name}" exited early:\n${output}`);
        const ready = await fetch(`http://127.0.0.1:${api}/api/v1/setup/status`).then(
            (response) => response.ok,
            () => false,
        );
        if (ready) return node;
        await sleep(200);
    }
    throw new Error(`the daemon "${name}" did not answer within 60 s:\n${output}`);
}

async function stopNode(node: Node) {
    if (node.child.exitCode !== null) return;
    const exited = new Promise<void>((resolve) => node.child.once("exit", () => resolve()));
    node.child.kill("SIGTERM");
    const timer = setTimeout(() => node.child.kill("SIGKILL"), 5000);
    await exited;
    clearTimeout(timer);
}

// Starts two daemons that share one network: the second bootstraps from the first. The first is reached as
// 127.0.0.1 and the second as localhost, so a browser keeps a separate cookie for each.
export default async function globalSetup() {
    if (!existsSync(daemonBin))
        throw new Error(`the daemon binary is missing: ${daemonBin}. Build it, or set AXUS_DAEMON_BIN.`);
    if (!existsSync(path.join(uiWebDir, "dist", "index.html")))
        throw new Error("the client build is missing: run `npm run build` first.");

    const root = mkdtempSync(path.join(os.tmpdir(), "axus-e2e-"));
    const nodes: Node[] = [];
    const teardown = async () => {
        await Promise.all(nodes.map(stopNode));
        if (!process.env.AXUS_E2E_KEEP) rmSync(root, { recursive: true, force: true });
    };
    try {
        const libraryRoot = path.join(root, "library");
        mkdirSync(libraryRoot);
        writeFileSync(path.join(libraryRoot, LIBRARY_FILE_NAME), LIBRARY_FILE_TEXT);

        const first = await startNode(root, "first", { libraryRoot }, nodes);
        const profile = /node_profile\S*?"(axus:node\/[^"]+)"/.exec(first.output())?.[1];
        if (!profile) throw new Error(`the first daemon did not log its node profile:\n${first.output()}`);
        const second = await startNode(root, "second", { bootstrap: profile }, nodes);
        process.env.AXUS_E2E_FIRST_URL = `http://127.0.0.1:${first.api}`;
        process.env.AXUS_E2E_SECOND_URL = `http://localhost:${second.api}`;
        process.env.AXUS_E2E_ROOT = root;
    } catch (error) {
        await teardown();
        throw error;
    }
    return teardown;
}
