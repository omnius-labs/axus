import { useQueryClient } from "@tanstack/react-query";
import { Link, Outlet, useNavigate } from "@tanstack/react-router";
import { api } from "./api";
import { messages } from "./messages";

const linkClass = "[&.active]:underline";

export function AppLayout() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const logout = async () => {
    await api.DELETE("/auth/session");
    queryClient.clear();
    await navigate({ to: "/login", search: {} });
  };
  return (
    <main className="mx-auto flex max-w-4xl flex-col gap-6 p-8">
      <header className="flex items-center gap-4 border-b pb-3">
        <h1 className="text-lg font-semibold">{messages.appName}</h1>
        <Link to="/publications" className={linkClass}>
          {messages.nav.publications}
        </Link>
        <Link to="/subscriptions" className={linkClass}>
          {messages.nav.subscriptions}
        </Link>
        <button type="button" className="ml-auto" onClick={logout}>
          {messages.nav.logout}
        </button>
      </header>
      <Outlet />
    </main>
  );
}

export function NotFound() {
  return (
    <div className="mx-auto mt-24 flex w-80 flex-col gap-3">
      <p>{messages.errors.pageNotFound}</p>
      <Link to="/publications" className="underline">
        {messages.nav.publications}
      </Link>
    </div>
  );
}
