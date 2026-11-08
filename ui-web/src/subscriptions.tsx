import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { api, isNotFound, unwrap } from "./api";
import { messages } from "./messages";
import { isActiveSubscription, isTerminalSubscription, itemRefetchInterval, POLL_LIST_MS } from "./status";
import { buttonClass, dangerClass, ErrorText, Field, formatTime, ProgressBar } from "./ui";

const subscriptionsKey = ["subscriptions"] as const;

function useSubscription(id: string) {
    return useQuery({
        queryKey: ["subscription", id],
        queryFn: async () =>
            unwrap(
                await api.GET("/subscriptions/{subscriptionId}", { params: { path: { subscriptionId: id } } }),
                messages.errors.generic,
            ),
        refetchInterval: (query) =>
            isNotFound(query.state.error)
                ? false
                : itemRefetchInterval(!!query.state.data && isTerminalSubscription(query.state.data.status)),
    });
}

export function SubscriptionsScreen() {
    const queryClient = useQueryClient();
    const navigate = useNavigate();
    const [rootHash, setRootHash] = useState("");
    const [fileName, setFileName] = useState("");
    const { data, error } = useQuery({
        queryKey: subscriptionsKey,
        queryFn: async () => unwrap(await api.GET("/subscriptions"), messages.errors.generic).subscriptions,
        refetchInterval: POLL_LIST_MS,
    });
    const create = useMutation({
        mutationFn: async () =>
            unwrap(
                await api.POST("/subscriptions", { body: { rootHash: rootHash.trim(), fileName: fileName.trim() } }),
                messages.subscriptions.createFailed,
            ),
        onSuccess: async (created) => {
            await queryClient.invalidateQueries({ queryKey: subscriptionsKey });
            await navigate({ to: "/subscriptions/$subscriptionId", params: { subscriptionId: created.id } });
        },
    });
    return (
        <section className="flex flex-col gap-4">
            <h2 className="text-lg font-semibold">{messages.subscriptions.title}</h2>
            <form
                className="flex gap-2"
                onSubmit={(event) => {
                    event.preventDefault();
                    create.mutate();
                }}
            >
                <input
                    className="flex-1 border p-2 font-mono"
                    value={rootHash}
                    onChange={(event) => setRootHash(event.target.value)}
                    placeholder={messages.subscriptions.rootHash}
                    aria-label={messages.subscriptions.rootHash}
                />
                <input
                    className="border p-2"
                    value={fileName}
                    onChange={(event) => setFileName(event.target.value)}
                    placeholder={messages.subscriptions.fileName}
                    aria-label={messages.subscriptions.fileName}
                />
                <button
                    className={buttonClass}
                    type="submit"
                    disabled={create.isPending || !rootHash.trim() || !fileName.trim()}
                >
                    {messages.subscriptions.subscribe}
                </button>
            </form>
            <ErrorText error={create.error ?? error} />
            {data?.length === 0 && <p>{messages.subscriptions.empty}</p>}
            <table className="w-full border-collapse text-left">
                <thead>
                    <tr className="border-b">
                        <th>{messages.common.name}</th>
                        <th>{messages.common.rootHash}</th>
                        <th>{messages.common.status}</th>
                        <th>{messages.subscriptions.blocks}</th>
                        <th>{messages.common.createdAt}</th>
                    </tr>
                </thead>
                <tbody>
                    {data?.map((item) => (
                        <tr key={item.id} className="border-b">
                            <td>
                                <Link
                                    to="/subscriptions/$subscriptionId"
                                    params={{ subscriptionId: item.id }}
                                    className="underline"
                                >
                                    {item.fileName}
                                </Link>
                            </td>
                            <td className="max-w-48 truncate font-mono text-xs">{item.rootHash}</td>
                            <td>{item.status}</td>
                            <td>
                                {item.blockCountDownloaded}/{item.blockCountTotal}
                            </td>
                            <td>{formatTime(item.createdAt)}</td>
                        </tr>
                    ))}
                </tbody>
            </table>
        </section>
    );
}

export function SubscriptionDetailScreen({ id }: { id: string }) {
    const navigate = useNavigate();
    const queryClient = useQueryClient();
    const { data, error } = useSubscription(id);
    const cancel = useMutation({
        mutationFn: async () =>
            unwrap(
                await api.POST("/subscriptions/{subscriptionId}/cancel", { params: { path: { subscriptionId: id } } }),
                messages.subscriptions.cancelFailed,
            ),
        onSuccess: async () => {
            await queryClient.invalidateQueries({ queryKey: subscriptionsKey });
            await queryClient.invalidateQueries({ queryKey: ["subscription", id] });
        },
    });
    const remove = useMutation({
        mutationFn: async () => {
            const { response, error } = await api.DELETE("/subscriptions/{subscriptionId}", {
                params: { path: { subscriptionId: id } },
            });
            if (!response.ok) throw new Error(error?.message ?? messages.subscriptions.deleteFailed);
        },
        onSuccess: async () => {
            queryClient.removeQueries({ queryKey: ["subscription", id] });
            await queryClient.invalidateQueries({ queryKey: subscriptionsKey });
            await navigate({ to: "/subscriptions" });
        },
    });

    if (isNotFound(error)) {
        return (
            <section className="flex flex-col gap-3">
                <ErrorText error={error} />
                <Link to="/subscriptions" className="underline">
                    {messages.common.back}
                </Link>
            </section>
        );
    }
    if (!data)
        return (
            <section>
                <ErrorText error={error} />
                {!error && <p>{messages.common.loading}</p>}
            </section>
        );

    return (
        <section className="flex flex-col gap-4">
            <div className="flex items-center gap-3">
                <Link to="/subscriptions" className="underline">
                    {messages.common.back}
                </Link>
                <h2 className="text-lg font-semibold">{data.fileName}</h2>
            </div>
            <dl>
                <Field label={messages.common.status}>{data.status}</Field>
                <Field label={messages.subscriptions.blocks}>
                    <div className="flex items-center gap-3">
                        <ProgressBar done={data.blockCountDownloaded} total={data.blockCountTotal} />
                        <span>
                            {data.blockCountDownloaded}/{data.blockCountTotal}
                        </span>
                    </div>
                </Field>
                <Field label={messages.common.rootHash}>
                    <span className="font-mono text-xs">{data.rootHash}</span>
                </Field>
                {data.failedReason && (
                    <Field label={messages.subscriptions.failedReason}>
                        <span className="text-red-600">{data.failedReason}</span>
                    </Field>
                )}
                <Field label={messages.common.createdAt}>{formatTime(data.createdAt)}</Field>
                <Field label={messages.common.updatedAt}>{formatTime(data.updatedAt)}</Field>
            </dl>
            <div className="flex gap-2">
                {data.status === "Completed" && (
                    <a className={buttonClass} href={`/api/v1/subscriptions/${data.id}/content`}>
                        {messages.subscriptions.download}
                    </a>
                )}
                {isActiveSubscription(data.status) && (
                    <button
                        type="button"
                        className={dangerClass}
                        disabled={cancel.isPending}
                        onClick={() => cancel.mutate()}
                    >
                        {messages.common.cancel}
                    </button>
                )}
                {isTerminalSubscription(data.status) && (
                    <button
                        type="button"
                        className={dangerClass}
                        disabled={remove.isPending}
                        onClick={() => window.confirm(messages.subscriptions.deleteConfirm) && remove.mutate()}
                    >
                        {messages.common.delete}
                    </button>
                )}
            </div>
            <ErrorText error={cancel.error ?? remove.error} />
        </section>
    );
}
