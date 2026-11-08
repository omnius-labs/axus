import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { api, unwrap } from "./api";
import { messages } from "./messages";
import { ErrorText } from "./ui";

export function LibraryPicker() {
    const queryClient = useQueryClient();
    const navigate = useNavigate();
    const [rootId, setRootId] = useState("");
    const [dir, setDir] = useState("");
    const roots = useQuery({
        queryKey: ["library-roots"],
        queryFn: async () => unwrap(await api.GET("/library/roots"), messages.errors.generic).roots,
    });
    const entries = useQuery({
        queryKey: ["library-entries", rootId, dir],
        enabled: rootId !== "",
        queryFn: async () =>
            unwrap(
                await api.GET("/library/roots/{rootId}/entries", {
                    params: { path: { rootId }, query: { path: dir } },
                }),
                messages.errors.generic,
            ).entries,
    });
    const publish = useMutation({
        mutationFn: async (path: string) =>
            unwrap(
                await api.POST("/publications", { body: { source: { kind: "library", rootId, path } } }),
                messages.library.publishFailed,
            ),
        onSuccess: async (created) => {
            await queryClient.invalidateQueries({ queryKey: ["publications"] });
            await navigate({ to: "/publications/$publicationId", params: { publicationId: created.id } });
        },
    });
    const parent = dir.split("/").slice(0, -1).join("/");
    return (
        <div className="flex flex-col gap-2 border p-3">
            <div className="flex gap-2">
                <select
                    className="border p-2"
                    aria-label={messages.library.chooseRoot}
                    value={rootId}
                    onChange={(event) => {
                        setRootId(event.target.value);
                        setDir("");
                    }}
                >
                    <option value="">{messages.library.chooseRoot}</option>
                    {roots.data?.map((root) => (
                        <option key={root.id} value={root.id}>
                            {root.name}
                        </option>
                    ))}
                </select>
                {dir && (
                    <button type="button" onClick={() => setDir(parent)}>
                        {messages.library.up}
                    </button>
                )}
            </div>
            {rootId && <p className="font-mono text-xs">/{dir}</p>}
            <ul>
                {entries.data?.map((entry) => (
                    <li key={entry.name}>
                        {entry.kind === "directory" ? (
                            <button
                                type="button"
                                className="underline"
                                onClick={() => setDir(dir ? `${dir}/${entry.name}` : entry.name)}
                            >
                                {entry.name}/
                            </button>
                        ) : (
                            <button
                                type="button"
                                className="underline"
                                disabled={publish.isPending}
                                onClick={() => publish.mutate(dir ? `${dir}/${entry.name}` : entry.name)}
                            >
                                {entry.name} ({entry.size ?? 0} B)
                            </button>
                        )}
                    </li>
                ))}
            </ul>
            <ErrorText error={roots.error ?? entries.error ?? publish.error} />
        </div>
    );
}
