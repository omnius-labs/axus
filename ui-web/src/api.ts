import createClient, { type Middleware } from "openapi-fetch";
import type { components, paths } from "./api/schema";
import { messages } from "./messages";

export type Publication = components["schemas"]["Publication"];
export type Subscription = components["schemas"]["Subscription"];
export type ErrorResponse = components["schemas"]["ErrorResponse"];

export const api = createClient<paths>({ baseUrl: "/api/v1" });

let onUnauthorized: () => void = () => {};

export function setUnauthorizedHandler(handler: () => void) {
    onUnauthorized = handler;
}

// A 401 on the login and setup calls means wrong credentials, not an expired session.
const unauthenticatedPaths = ["/auth/session", "/setup"];

export function reportUnauthorized(path: string) {
    if (!unauthenticatedPaths.some((prefix) => path.startsWith(`/api/v1${prefix}`))) onUnauthorized();
}

const sessionGuard: Middleware = {
    onResponse({ request, response }) {
        if (response.status === 401) reportUnauthorized(new URL(request.url).pathname);
    },
};

api.use(sessionGuard);

export class ApiError extends Error {
    constructor(
        readonly status: number,
        message: string,
        readonly receivedBytes?: number,
    ) {
        super(message);
    }
}

export async function throwIfFailed(response: Response, fallback: string) {
    if (response.ok) return;
    const body = (await response.json().catch(() => undefined)) as ErrorResponse | undefined;
    throw new ApiError(response.status, body?.message ?? fallback, body?.receivedBytes);
}

export function describeError(error: unknown, fallback: string = messages.errors.generic) {
    if (error instanceof ApiError) {
        if (error.status === 403) return messages.errors.forbidden;
        if (error.status === 404) return messages.errors.notFound;
        if (error.status >= 500) return messages.errors.server;
        return error.message || fallback;
    }
    return error instanceof Error ? error.message : fallback;
}

type Result<T> = { data?: T; error?: ErrorResponse; response: Response };

export function unwrap<T>({ data, error, response }: Result<T>, fallback: string): T {
    if (!response.ok || data === undefined)
        throw new ApiError(response.status, error?.message ?? fallback, error?.receivedBytes);
    return data;
}

export function isNotFound(error: unknown) {
    return error instanceof ApiError && error.status === 404;
}
