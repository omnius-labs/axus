import { useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { api } from "./api";
import { messages } from "./messages";
import { safeRedirect } from "./status";

export const setupStatusQuery = {
  queryKey: ["setup"],
  queryFn: async () => (await api.GET("/setup/status")).data?.initialized ?? false,
};

const inputClass = "border p-2";
const buttonClass = "bg-black p-2 text-white";

export function SetupScreen() {
  const navigate = useNavigate();
  const [name, setName] = useState("owner");
  const [password, setPassword] = useState("");
  const [again, setAgain] = useState("");
  const [error, setError] = useState("");
  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError("");
    if (password !== again) {
      setError(messages.auth.mismatch);
      return;
    }
    const created = await api.POST("/setup", { body: { name, password } });
    if (!created.response.ok) {
      setError(created.error?.message ?? messages.auth.setupFailed);
      return;
    }
    const session = await api.POST("/auth/session", { body: { name, password } });
    if (!session.response.ok) {
      setError(messages.auth.loginFailed);
      return;
    }
    await navigate({ to: "/publications" });
  };
  return (
    <form onSubmit={submit} className="mx-auto mt-24 flex w-80 flex-col gap-3">
      <h1 className="text-xl font-semibold">{messages.auth.setupTitle}</h1>
      <input
        className={inputClass}
        value={name}
        onChange={(event) => setName(event.target.value)}
        placeholder={messages.auth.name}
        aria-label={messages.auth.name}
        autoComplete="username"
      />
      <input
        className={inputClass}
        type="password"
        value={password}
        onChange={(event) => setPassword(event.target.value)}
        placeholder={messages.auth.password}
        aria-label="Secret"
        autoComplete="new-password"
      />
      <input
        className={inputClass}
        type="password"
        value={again}
        onChange={(event) => setAgain(event.target.value)}
        placeholder={messages.auth.passwordAgain}
        aria-label={messages.auth.passwordAgain}
        autoComplete="new-password"
      />
      <button className={buttonClass} type="submit">
        {messages.auth.createOwner}
      </button>
      {error && (
        <p role="alert" className="text-red-600">
          {error}
        </p>
      )}
    </form>
  );
}

export function LoginScreen({ redirect }: { redirect?: string }) {
  const navigate = useNavigate();
  const [name, setName] = useState("owner");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError("");
    const { response } = await api.POST("/auth/session", { body: { name, password } });
    if (!response.ok) {
      setError(messages.auth.loginFailed);
      return;
    }
    await navigate({ href: safeRedirect(redirect) });
  };
  return (
    <form onSubmit={submit} className="mx-auto mt-24 flex w-80 flex-col gap-3">
      <h1 className="text-xl font-semibold">{messages.auth.loginTitle}</h1>
      <input
        className={inputClass}
        value={name}
        onChange={(event) => setName(event.target.value)}
        placeholder={messages.auth.name}
        aria-label={messages.auth.name}
        autoComplete="username"
      />
      <input
        className={inputClass}
        type="password"
        value={password}
        onChange={(event) => setPassword(event.target.value)}
        placeholder={messages.auth.password}
        aria-label={messages.auth.password}
        autoComplete="current-password"
      />
      <button className={buttonClass} type="submit">
        {messages.auth.login}
      </button>
      {error && (
        <p role="alert" className="text-red-600">
          {error}
        </p>
      )}
    </form>
  );
}
