import type { Publication, Subscription } from "./api";

export type PublicationStatus = Publication["status"];
export type SubscriptionStatus = Subscription["status"];

const activePublication: readonly PublicationStatus[] = ["Receiving", "Pending", "Processing"];
const activeSubscription: readonly SubscriptionStatus[] = ["Downloading", "Decoding"];

// Active rows may be canceled and terminal rows may be deleted. Finalizing is neither:
// placement is already running, and the daemon answers 409 to both. A row is polled until it is terminal.
export const isActivePublication = (status: PublicationStatus) => activePublication.includes(status);
export const isActiveSubscription = (status: SubscriptionStatus) => activeSubscription.includes(status);
export const isTerminalSubscription = (status: SubscriptionStatus) =>
    status === "Completed" || status === "Failed" || status === "Canceled";
export const isTerminalPublication = (status: PublicationStatus) =>
    status === "Committed" || status === "Failed" || status === "Canceled";

export const POLL_ITEM_MS = 1000;
export const POLL_LIST_MS = 5000;

export function itemRefetchInterval(terminal: boolean) {
    return terminal ? false : POLL_ITEM_MS;
}

export function percent(done: number, total: number) {
    return total === 0 ? 0 : Math.min(100, Math.floor((done / total) * 100));
}

// Returns a same-origin path or the fallback, so a crafted redirect cannot leave the site.
export function safeRedirect(value: unknown, fallback = "/publications") {
    if (typeof value !== "string") return fallback;
    if (!value.startsWith("/") || value.startsWith("//") || value.includes("\\")) return fallback;
    return value;
}
