import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";

export function LibraryPicker() {
  const queryClient = useQueryClient();
  const [rootId, setRootId] = useState("");
  const [dir, setDir] = useState("");
  const roots = useQuery({
    queryKey: ["library-roots"],
    queryFn: async () => (await api.GET("/library/roots")).data?.roots ?? [],
  });
  const entries = useQuery({
    queryKey: ["library-entries", rootId, dir],
    enabled: rootId !== "",
    queryFn: async () =>
      (await api.GET("/library/roots/{rootId}/entries", { params: { path: { rootId }, query: { path: dir } } })).data?.entries ?? [],
  });
  const publish = useMutation({
    mutationFn: async (path: string) => {
      const { response } = await api.POST("/publications", { body: { source: { kind: "library", rootId, path } } });
      if (!response.ok) throw new Error("could not publish the library file");
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["publications"] }),
  });
  const parent = dir.split("/").slice(0, -1).join("/");
  return (
    <div className="flex flex-col gap-2 border p-3">
      <div className="flex gap-2">
        <select
          className="border p-2"
          value={rootId}
          onChange={(event) => {
            setRootId(event.target.value);
            setDir("");
          }}
        >
          <option value="">Choose a library root</option>
          {roots.data?.map((root) => (
            <option key={root.id} value={root.id}>{root.name}</option>
          ))}
        </select>
        {dir && <button type="button" onClick={() => setDir(parent)}>up</button>}
      </div>
      {rootId && <p className="font-mono text-xs">/{dir}</p>}
      <ul>
        {entries.data?.map((entry) => (
          <li key={entry.name}>
            {entry.kind === "directory" ? (
              <button type="button" className="underline" onClick={() => setDir(dir ? `${dir}/${entry.name}` : entry.name)}>
                {entry.name}/
              </button>
            ) : (
              <button type="button" className="underline" onClick={() => publish.mutate(dir ? `${dir}/${entry.name}` : entry.name)}>
                {entry.name} ({entry.size ?? 0} B)
              </button>
            )}
          </li>
        ))}
      </ul>
      {publish.error && <p className="text-red-600">{String(publish.error)}</p>}
    </div>
  );
}
