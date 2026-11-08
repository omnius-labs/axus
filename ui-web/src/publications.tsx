import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";
import { api, isNotFound, type Publication, unwrap } from "./api";
import { LibraryPicker } from "./library";
import { messages } from "./messages";
import { isActivePublication, isTerminalPublication, itemRefetchInterval, POLL_LIST_MS } from "./status";
import { buttonClass, CopyBox, dangerClass, ErrorText, Field, formatTime, ProgressBar } from "./ui";
import { uploadContent } from "./upload";

const publicationsKey = ["publications"] as const;

export function usePublicationList() {
    return useQuery({
        queryKey: publicationsKey,
        queryFn: async () => unwrap(await api.GET("/publications"), messages.errors.generic).publications,
        refetchInterval: POLL_LIST_MS,
    });
}

function usePublication(id: string) {
    return useQuery({
        queryKey: ["publication", id],
        queryFn: async () =>
            unwrap(
                await api.GET("/publications/{publicationId}", { params: { path: { publicationId: id } } }),
                messages.errors.generic,
            ),
        refetchInterval: (query) =>
            isNotFound(query.state.error)
                ? false
                : itemRefetchInterval(!!query.state.data && isTerminalPublication(query.state.data.status)),
    });
}

function shareText(item: Publication) {
    return `${messages.common.rootHash}: ${item.rootHash}\n${messages.common.name}: ${item.fileName}`;
}

// Sends a file from the stored offset while the page is open. A page unload warns the user,
// and the publication stays `Receiving` so a later visit resumes it.
function useUploader() {
    const queryClient = useQueryClient();
    const controller = useRef<AbortController | null>(null);
    const [uploading, setUploading] = useState(false);
    const [error, setError] = useState<unknown>(null);
    const [received, setReceived] = useState(0);

    useEffect(() => {
        if (!uploading) return;
        const warn = (event: BeforeUnloadEvent) => {
            event.preventDefault();
            event.returnValue = messages.publications.leaveWarning;
        };
        window.addEventListener("beforeunload", warn);
        return () => window.removeEventListener("beforeunload", warn);
    }, [uploading]);

    useEffect(() => () => controller.current?.abort(), []);

    const upload = async (id: string, file: File) => {
        controller.current?.abort();
        const current = new AbortController();
        controller.current = current;
        setError(null);
        setUploading(true);
        try {
            await uploadContent({ id, file, signal: current.signal, onProgress: setReceived });
            return true;
        } catch (cause) {
            if (!current.signal.aborted) setError(cause);
            return false;
        } finally {
            setUploading(false);
            await queryClient.invalidateQueries({ queryKey: publicationsKey });
        }
    };
    return { upload, uploading, error, received };
}

export function PublicationsScreen() {
    const { data, error } = usePublicationList();
    return (
        <section className="flex flex-col gap-4">
            <div className="flex items-center justify-between">
                <h2 className="text-lg font-semibold">{messages.publications.title}</h2>
                <Link to="/publications/new" className={buttonClass}>
                    {messages.publications.new}
                </Link>
            </div>
            <ErrorText error={error} />
            {data?.length === 0 && <p>{messages.publications.empty}</p>}
            <table className="w-full border-collapse text-left">
                <thead>
                    <tr className="border-b">
                        <th>{messages.common.name}</th>
                        <th>{messages.common.status}</th>
                        <th>{messages.common.progress}</th>
                        <th>{messages.common.rootHash}</th>
                        <th>{messages.common.createdAt}</th>
                    </tr>
                </thead>
                <tbody>
                    {data?.map((item) => (
                        <tr key={item.id} className="border-b">
                            <td>
                                <Link
                                    to="/publications/$publicationId"
                                    params={{ publicationId: item.id }}
                                    className="underline"
                                >
                                    {item.fileName}
                                </Link>
                            </td>
                            <td>{item.status}</td>
                            <td>
                                {item.status === "Receiving" && (
                                    <ProgressBar done={item.receivedBytes} total={item.size} />
                                )}
                            </td>
                            <td className="max-w-48 truncate font-mono text-xs">{item.rootHash ?? ""}</td>
                            <td>{formatTime(item.createdAt)}</td>
                        </tr>
                    ))}
                </tbody>
            </table>
        </section>
    );
}

export function NewPublicationScreen() {
    const navigate = useNavigate();
    const queryClient = useQueryClient();
    const [file, setFile] = useState<File | null>(null);
    const uploader = useUploader();
    const create = useMutation({
        mutationFn: async (chosen: File) => {
            const created = unwrap(
                await api.POST("/publications", { body: { fileName: chosen.name, size: chosen.size } }),
                messages.publications.createFailed,
            );
            await queryClient.invalidateQueries({ queryKey: publicationsKey });
            if (await uploader.upload(created.id, chosen))
                await navigate({ to: "/publications/$publicationId", params: { publicationId: created.id } });
        },
    });
    const busy = create.isPending || uploader.uploading;
    return (
        <section className="flex flex-col gap-6">
            <h2 className="text-lg font-semibold">{messages.publications.new}</h2>
            <div className="flex flex-col gap-2 border p-3">
                <h3 className="font-medium">{messages.publications.upload}</h3>
                <div className="flex gap-2">
                    <input
                        type="file"
                        aria-label={messages.publications.upload}
                        onChange={(event) => setFile(event.target.files?.[0] ?? null)}
                    />
                    <button
                        type="button"
                        className={buttonClass}
                        disabled={!file || busy}
                        onClick={() => file && create.mutate(file)}
                    >
                        {busy ? messages.publications.uploading : messages.publications.publish}
                    </button>
                </div>
                {file && uploader.uploading && <ProgressBar done={uploader.received} total={file.size} />}
                <ErrorText error={create.error ?? uploader.error} />
            </div>
            <div className="flex flex-col gap-2">
                <h3 className="font-medium">{messages.publications.library}</h3>
                <LibraryPicker />
            </div>
        </section>
    );
}

export function PublicationDetailScreen({ id }: { id: string }) {
    const navigate = useNavigate();
    const queryClient = useQueryClient();
    const { data, error } = usePublication(id);
    const uploader = useUploader();
    const [resumeError, setResumeError] = useState("");
    const done = async () => {
        await queryClient.invalidateQueries({ queryKey: publicationsKey });
        await queryClient.invalidateQueries({ queryKey: ["publication", id] });
    };
    const cancel = useMutation({
        mutationFn: async () =>
            unwrap(
                await api.POST("/publications/{publicationId}/cancel", { params: { path: { publicationId: id } } }),
                messages.publications.cancelFailed,
            ),
        onSuccess: done,
    });
    const remove = useMutation({
        mutationFn: async () => {
            const { response, error } = await api.DELETE("/publications/{publicationId}", {
                params: { path: { publicationId: id } },
            });
            if (!response.ok) throw new Error(error?.message ?? messages.publications.deleteFailed);
        },
        onSuccess: async () => {
            queryClient.removeQueries({ queryKey: ["publication", id] });
            await queryClient.invalidateQueries({ queryKey: publicationsKey });
            await navigate({ to: "/publications" });
        },
    });

    if (isNotFound(error)) {
        return (
            <section className="flex flex-col gap-3">
                <ErrorText error={error} />
                <Link to="/publications" className="underline">
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

    const resume = (chosen: File | undefined) => {
        setResumeError("");
        if (!chosen) return;
        if (chosen.name !== data.fileName || chosen.size !== data.size) {
            setResumeError(messages.publications.resumeMismatch);
            return;
        }
        void uploader.upload(id, chosen);
    };
    const confirmDelete = () => {
        if (data.status !== "Committed" || window.confirm(messages.publications.deleteCommitted)) remove.mutate();
    };
    return (
        <section className="flex flex-col gap-4">
            <div className="flex items-center gap-3">
                <Link to="/publications" className="underline">
                    {messages.common.back}
                </Link>
                <h2 className="text-lg font-semibold">{data.fileName}</h2>
            </div>
            <dl>
                <Field label={messages.common.status}>{data.status}</Field>
                <Field label={messages.common.progress}>
                    <ProgressBar done={data.receivedBytes} total={data.size} />
                </Field>
                {data.rootHash && (
                    <Field label={messages.common.rootHash}>
                        <span className="font-mono text-xs">{data.rootHash}</span>
                    </Field>
                )}
                {data.failedReason && (
                    <Field label={messages.publications.failedReason}>
                        <span className="text-red-600">{data.failedReason}</span>
                    </Field>
                )}
                <Field label={messages.common.createdAt}>{formatTime(data.createdAt)}</Field>
                <Field label={messages.common.updatedAt}>{formatTime(data.updatedAt)}</Field>
            </dl>
            {data.rootHash && (
                <div className="flex flex-col gap-1">
                    <h3 className="font-medium">{messages.publications.shareText}</h3>
                    <CopyBox text={shareText(data)} />
                </div>
            )}
            {data.status === "Receiving" && (
                <div className="flex flex-col gap-1">
                    <p>{messages.publications.resumeHint}</p>
                    <input
                        type="file"
                        aria-label={messages.publications.resume}
                        disabled={uploader.uploading}
                        onChange={(event) => resume(event.target.files?.[0])}
                    />
                    {resumeError && (
                        <p role="alert" className="text-red-600">
                            {resumeError}
                        </p>
                    )}
                    <ErrorText error={uploader.error} />
                </div>
            )}
            <div className="flex gap-2">
                {isActivePublication(data.status) && (
                    <button
                        type="button"
                        className={dangerClass}
                        disabled={cancel.isPending}
                        onClick={() => cancel.mutate()}
                    >
                        {messages.common.cancel}
                    </button>
                )}
                {isTerminalPublication(data.status) && (
                    <button type="button" className={dangerClass} disabled={remove.isPending} onClick={confirmDelete}>
                        {messages.common.delete}
                    </button>
                )}
            </div>
            <ErrorText error={cancel.error ?? remove.error} />
        </section>
    );
}
