import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";

type View = "publications" | "subscriptions" | "login";

type Publication = {
  id: string;
  fileName: string;
  size: number;
  receivedBytes: number;
  status: string;
  rootHash?: string;
};

type Subscription = {
  id: string;
  rootHash: string;
  fileName: string;
  status: string;
  blockCountDownloaded: number;
  blockCountTotal: number;
};

async function login(name: string, password: string) {
  const { response } = await api.POST("/auth/session", { body: { name, password } });
  if (!response.ok) throw new Error("login failed");
}

async function setup(name: string, password: string) {
  const { response } = await api.POST("/setup", { body: { name, password } });
  if (!response.ok) throw new Error("setup failed");
}

function Login({ onDone }: { onDone: () => void }) {
  const [name, setName] = useState("owner");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const status = useQuery({
    queryKey: ["setup"],
    queryFn: async () => (await api.GET("/setup/status")).data?.initialized ?? false,
  });
  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError("");
    try {
      if (!status.data) await setup(name, password);
      await login(name, password);
      onDone();
    } catch (error) {
      setError(error instanceof Error ? error.message : "failed");
    }
  };
  return (
    <form onSubmit={submit} className="mx-auto mt-24 flex w-80 flex-col gap-3">
      <h1 className="text-xl font-semibold">{status.data ? "Log in" : "Set up the owner"}</h1>
      <input className="border p-2" value={name} onChange={(event) => setName(event.target.value)} placeholder="name" />
      <input className="border p-2" type="password" value={password} onChange={(event) => setPassword(event.target.value)} placeholder="password" />
      <button className="bg-black p-2 text-white" type="submit">{status.data ? "Log in" : "Create owner"}</button>
      {error && <p className="text-red-600">{error}</p>}
    </form>
  );
}

function Publications() {
  const queryClient = useQueryClient();
  const [file, setFile] = useState<File | null>(null);
  const { data } = useQuery({
    queryKey: ["publications"],
    queryFn: async () => ((await api.GET("/publications")).data?.publications ?? []) as Publication[],
    refetchInterval: 1000,
  });
  const upload = useMutation({
    mutationFn: async (file: File) => {
      const created = await api.POST("/publications", { body: { fileName: file.name, size: file.size } });
      if (!created.data) throw new Error("could not create the publication");
      const id = created.data.id;
      const chunk = 8 * 1024 * 1024;
      let offset = 0;
      while (offset < file.size) {
        const end = Math.min(offset + chunk, file.size);
        const response = await fetch(`/api/v1/publications/${id}/content`, {
          method: "PUT",
          headers: { "content-range": `bytes ${offset}-${end - 1}/${file.size}` },
          body: file.slice(offset, end),
        });
        if (response.status === 409) {
          offset = (await response.json()).receivedBytes as number;
          continue;
        }
        if (!response.ok) throw new Error("upload failed");
        offset = end;
      }
      return id;
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["publications"] }),
  });
  return (
    <section className="flex flex-col gap-4">
      <div className="flex gap-2">
        <input type="file" onChange={(event) => setFile(event.target.files?.[0] ?? null)} />
        <button className="bg-black px-3 text-white" disabled={!file || upload.isPending} onClick={() => file && upload.mutate(file)}>
          {upload.isPending ? "Uploading" : "Publish"}
        </button>
      </div>
      {upload.error && <p className="text-red-600">{String(upload.error)}</p>}
      <table className="w-full border-collapse text-left">
        <thead>
          <tr className="border-b"><th>name</th><th>status</th><th>progress</th><th>root hash</th></tr>
        </thead>
        <tbody>
          {data?.map((item) => (
            <tr key={item.id} className="border-b">
              <td>{item.fileName}</td>
              <td>{item.status}</td>
              <td>{item.size === 0 ? "" : `${Math.floor((item.receivedBytes / item.size) * 100)}%`}</td>
              <td className="font-mono text-xs">{item.rootHash ?? ""}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

function Subscriptions() {
  const queryClient = useQueryClient();
  const [rootHash, setRootHash] = useState("");
  const [fileName, setFileName] = useState("");
  const { data } = useQuery({
    queryKey: ["subscriptions"],
    queryFn: async () => ((await api.GET("/subscriptions")).data?.subscriptions ?? []) as Subscription[],
    refetchInterval: 1000,
  });
  const create = useMutation({
    mutationFn: async () => {
      const { response } = await api.POST("/subscriptions", { body: { rootHash, fileName } });
      if (!response.ok) throw new Error("could not start the subscription");
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["subscriptions"] }),
  });
  return (
    <section className="flex flex-col gap-4">
      <form
        className="flex gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          create.mutate();
        }}
      >
        <input className="flex-1 border p-2 font-mono" value={rootHash} onChange={(event) => setRootHash(event.target.value)} placeholder="root hash" />
        <input className="border p-2" value={fileName} onChange={(event) => setFileName(event.target.value)} placeholder="file name" />
        <button className="bg-black px-3 text-white" type="submit">Subscribe</button>
      </form>
      {create.error && <p className="text-red-600">{String(create.error)}</p>}
      <table className="w-full border-collapse text-left">
        <thead>
          <tr className="border-b"><th>name</th><th>status</th><th>blocks</th><th></th></tr>
        </thead>
        <tbody>
          {data?.map((item) => (
            <tr key={item.id} className="border-b">
              <td>{item.fileName}</td>
              <td>{item.status}</td>
              <td>{item.blockCountDownloaded}/{item.blockCountTotal}</td>
              <td>{item.status === "Completed" && <a className="underline" href={`/api/v1/subscriptions/${item.id}/content`}>download</a>}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

export function App() {
  const [view, setView] = useState<View>("publications");
  const [authenticated, setAuthenticated] = useState(true);
  useEffect(() => {
    api.GET("/publications").then(({ response }) => setAuthenticated(response.status !== 401));
  }, []);
  if (!authenticated || view === "login") {
    return <Login onDone={() => { setAuthenticated(true); setView("publications"); }} />;
  }
  return (
    <main className="mx-auto flex max-w-4xl flex-col gap-6 p-8">
      <header className="flex items-center gap-4 border-b pb-3">
        <h1 className="text-lg font-semibold">Axus</h1>
        <button className={view === "publications" ? "underline" : ""} onClick={() => setView("publications")}>Publications</button>
        <button className={view === "subscriptions" ? "underline" : ""} onClick={() => setView("subscriptions")}>Subscriptions</button>
        <button className="ml-auto" onClick={async () => { await api.DELETE("/auth/session"); setAuthenticated(false); }}>Log out</button>
      </header>
      {view === "publications" ? <Publications /> : <Subscriptions />}
    </main>
  );
}
