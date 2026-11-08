import { ApiError, type Publication, reportUnauthorized, throwIfFailed } from "./api";
import { messages } from "./messages";

export const CHUNK_BYTES = 8 * 1024 * 1024;
const BACKOFF_MIN_MS = 1000;
const BACKOFF_MAX_MS = 30_000;

type Sleep = (ms: number, signal?: AbortSignal) => Promise<void>;

export type UploadOptions = {
    id: string;
    file: Blob;
    signal?: AbortSignal;
    onProgress?: (receivedBytes: number) => void;
    sleep?: Sleep;
};

export function sleepWithSignal(ms: number, signal?: AbortSignal) {
    return new Promise<void>((resolve, reject) => {
        const timer = setTimeout(resolve, ms);
        signal?.addEventListener(
            "abort",
            () => {
                clearTimeout(timer);
                reject(signal.reason);
            },
            { once: true },
        );
    });
}

const contentPath = (id: string) => `/api/v1/publications/${id}/content`;

// The daemon's offset comes from the resource itself, never from what the client sent.
async function storedBytes(id: string) {
    const response = await fetch(`/api/v1/publications/${id}`);
    if (response.status === 401) reportUnauthorized(`/api/v1/publications/${id}`);
    await throwIfFailed(response, messages.publications.uploadFailed);
    return ((await response.json()) as Publication).receivedBytes;
}

async function offsetAfterBackoff(id: string, offset: number, backoff: number, sleep: Sleep, signal?: AbortSignal) {
    await sleep(backoff, signal);
    try {
        return await storedBytes(id);
    } catch {
        return offset;
    }
}

// Sends the file in order from the offset the daemon reports. The daemon's offset is the only
// progress trusted: a conflict, a network error, or a 5xx continues from `receivedBytes`.
export async function uploadContent({ id, file, signal, onProgress, sleep = sleepWithSignal }: UploadOptions) {
    let offset = (await storedBytes(id)) ?? 0;
    let backoff = BACKOFF_MIN_MS;
    onProgress?.(offset);
    while (offset < file.size) {
        signal?.throwIfAborted();
        const end = Math.min(offset + CHUNK_BYTES, file.size);
        let response: Response | undefined;
        try {
            response = await fetch(contentPath(id), {
                method: "PUT",
                headers: {
                    "content-range": `bytes ${offset}-${end - 1}/${file.size}`,
                    "content-type": "application/octet-stream",
                },
                body: file.slice(offset, end),
                signal,
            });
        } catch (error) {
            if (signal?.aborted) throw error;
        }
        // 507 is a full destination, which a retry cannot fix, so it stops below instead of backing off.
        if (!response || (response.status >= 500 && response.status !== 507)) {
            offset = await offsetAfterBackoff(id, offset, backoff, sleep, signal);
            backoff = Math.min(backoff * 2, BACKOFF_MAX_MS);
            continue;
        }
        backoff = BACKOFF_MIN_MS;
        if (response.status === 401) reportUnauthorized(contentPath(id));
        if (response.status === 409) {
            const body = (await response.json().catch(() => undefined)) as { receivedBytes?: number } | undefined;
            const next = body?.receivedBytes ?? (await storedBytes(id));
            // A conflict that moves nowhere means the publication no longer accepts content.
            if (next === offset) throw new ApiError(409, messages.publications.uploadFailed, next);
            offset = next;
        } else if (response.status === 413) {
            throw new ApiError(413, messages.errors.tooLarge(file.size));
        } else if (response.status === 507) {
            throw new ApiError(507, messages.errors.insufficientStorage);
        } else {
            await throwIfFailed(response, messages.publications.uploadFailed);
            offset = ((await response.json()) as Publication).receivedBytes;
        }
        onProgress?.(offset);
    }
}
