import { afterEach, describe, expect, it, vi } from "vitest";
import { CHUNK_BYTES, uploadContent } from "./upload";

type Plan = { fail?: (call: number) => Response | "network" | undefined };

// A minimal daemon: it stores bytes in order and reports `receivedBytes` the way the API does.
function fakeDaemon(size: number, plan: Plan = {}) {
    let received = 0;
    let calls = 0;
    const ranges: string[] = [];
    const publication = () => ({ id: "p1", fileName: "a.bin", size, receivedBytes: received, status: "Receiving" });
    const handler = async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        if (!init?.method || init.method === "GET") return Response.json(publication());
        calls += 1;
        const injected = plan.fail?.(calls);
        if (injected === "network") throw new TypeError("network down");
        if (injected) return injected;
        const range = new Headers(init.headers).get("content-range") ?? "";
        ranges.push(range);
        const [, start, end] = /bytes (\d+)-(\d+)\//.exec(range) ?? [];
        expect(url).toBe("/api/v1/publications/p1/content");
        if (Number(start) !== received)
            return Response.json({ kind: "Conflict", receivedBytes: received }, { status: 409 });
        received = Number(end) + 1;
        return Response.json(publication());
    };
    return { handler, ranges, stored: () => received };
}

const noSleep = (_ms?: number) => Promise.resolve();

afterEach(() => vi.unstubAllGlobals());

describe("uploadContent", () => {
    it("sends 8 MiB chunks with Content-Range", async () => {
        const size = CHUNK_BYTES + 10;
        const daemon = fakeDaemon(size);
        vi.stubGlobal("fetch", daemon.handler);
        await uploadContent({ id: "p1", file: new Blob([new Uint8Array(size)]), sleep: noSleep });
        expect(daemon.ranges).toEqual([
            `bytes 0-${CHUNK_BYTES - 1}/${size}`,
            `bytes ${CHUNK_BYTES}-${size - 1}/${size}`,
        ]);
        expect(daemon.stored()).toBe(size);
    });

    it("sends no content request for a zero-byte file", async () => {
        const daemon = fakeDaemon(0);
        vi.stubGlobal("fetch", daemon.handler);
        await uploadContent({ id: "p1", file: new Blob([]), sleep: noSleep });
        expect(daemon.ranges).toEqual([]);
    });

    it("resumes from receivedBytes after a conflict", async () => {
        const size = 2 * CHUNK_BYTES;
        const daemon = fakeDaemon(size, {
            fail: (call) =>
                call === 1
                    ? Response.json({ kind: "Conflict", receivedBytes: CHUNK_BYTES }, { status: 409 })
                    : undefined,
        });
        vi.stubGlobal("fetch", daemon.handler);
        const file = new Blob([new Uint8Array(size)]);
        // The first PUT is answered with a conflict that points at the second chunk; the daemon never stored the first.
        await uploadContent({ id: "p1", file, sleep: noSleep }).catch(() => undefined);
        expect(daemon.ranges[0]).toBe(`bytes ${CHUNK_BYTES}-${size - 1}/${size}`);
    });

    it("retries a network error and a 5xx with a growing backoff", async () => {
        const daemon = fakeDaemon(10, {
            fail: (call) => (call === 1 ? "network" : call === 2 ? new Response("", { status: 503 }) : undefined),
        });
        vi.stubGlobal("fetch", daemon.handler);
        const sleep = vi.fn(noSleep);
        await uploadContent({ id: "p1", file: new Blob([new Uint8Array(10)]), sleep });
        expect(sleep.mock.calls.map(([ms]) => ms)).toEqual([1000, 2000]);
        expect(daemon.stored()).toBe(10);
    });

    it("stops on 413 and 507", async () => {
        for (const status of [413, 507]) {
            const daemon = fakeDaemon(10, { fail: () => new Response("", { status }) });
            vi.stubGlobal("fetch", daemon.handler);
            await expect(
                uploadContent({ id: "p1", file: new Blob([new Uint8Array(10)]), sleep: noSleep }),
            ).rejects.toMatchObject({ status });
        }
    });

    it("stops on a conflict that does not move", async () => {
        const daemon = fakeDaemon(10, {
            fail: () => Response.json({ kind: "Conflict", receivedBytes: 0 }, { status: 409 }),
        });
        vi.stubGlobal("fetch", daemon.handler);
        await expect(
            uploadContent({ id: "p1", file: new Blob([new Uint8Array(10)]), sleep: noSleep }),
        ).rejects.toMatchObject({ status: 409 });
    });

    it("stops when aborted", async () => {
        const daemon = fakeDaemon(10);
        vi.stubGlobal("fetch", daemon.handler);
        const controller = new AbortController();
        controller.abort();
        await expect(
            uploadContent({
                id: "p1",
                file: new Blob([new Uint8Array(10)]),
                signal: controller.signal,
                sleep: noSleep,
            }),
        ).rejects.toBeDefined();
        expect(daemon.ranges).toEqual([]);
    });
});
