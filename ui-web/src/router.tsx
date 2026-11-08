import { QueryClient } from "@tanstack/react-query";
import { createRootRoute, createRoute, createRouter, Outlet, redirect } from "@tanstack/react-router";
import { ApiError, setUnauthorizedHandler } from "./api";
import { AppLayout, NotFound } from "./app";
import { LoginScreen, SetupScreen, setupStatusQuery } from "./auth";
import { NewPublicationScreen, PublicationDetailScreen, PublicationsScreen } from "./publications";
import { safeRedirect } from "./status";
import { SubscriptionDetailScreen, SubscriptionsScreen } from "./subscriptions";

// A missing resource and a bad request are answered once; only a 5xx or a network error is retried.
export const queryClient = new QueryClient({
    defaultOptions: {
        queries: {
            retry: (count, error) => !(error instanceof ApiError && error.status < 500) && count < 5,
            retryDelay: (attempt) => Math.min(1000 * 2 ** attempt, 30_000),
        },
    },
});

const rootRoute = createRootRoute({ component: Outlet, notFoundComponent: NotFound });

const indexRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/",
    beforeLoad: () => {
        throw redirect({ to: "/publications" });
    },
});

const setupRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/setup",
    beforeLoad: async () => {
        if (await queryClient.fetchQuery({ ...setupStatusQuery, staleTime: 0 }))
            throw redirect({ to: "/login", search: {} });
    },
    component: SetupScreen,
});

const loginRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/login",
    validateSearch: (search: Record<string, unknown>): { redirect?: string } =>
        typeof search.redirect === "string" ? { redirect: safeRedirect(search.redirect) } : {},
    beforeLoad: async () => {
        if (!(await queryClient.fetchQuery({ ...setupStatusQuery, staleTime: 0 }))) throw redirect({ to: "/setup" });
    },
    component: function Login() {
        return <LoginScreen redirect={loginRoute.useSearch().redirect} />;
    },
});

const appRoute = createRoute({ getParentRoute: () => rootRoute, id: "app", component: AppLayout });

const publicationsRoute = createRoute({
    getParentRoute: () => appRoute,
    path: "/publications",
    component: PublicationsScreen,
});
// The literal `new` ranks above the `$publicationId` parameter.
const newPublicationRoute = createRoute({
    getParentRoute: () => appRoute,
    path: "/publications/new",
    component: NewPublicationScreen,
});
const publicationRoute = createRoute({
    getParentRoute: () => appRoute,
    path: "/publications/$publicationId",
    component: function PublicationDetail() {
        return <PublicationDetailScreen id={publicationRoute.useParams().publicationId} />;
    },
});

const subscriptionsRoute = createRoute({
    getParentRoute: () => appRoute,
    path: "/subscriptions",
    component: SubscriptionsScreen,
});
const subscriptionRoute = createRoute({
    getParentRoute: () => appRoute,
    path: "/subscriptions/$subscriptionId",
    component: function SubscriptionDetail() {
        return <SubscriptionDetailScreen id={subscriptionRoute.useParams().subscriptionId} />;
    },
});

const routeTree = rootRoute.addChildren([
    indexRoute,
    setupRoute,
    loginRoute,
    appRoute.addChildren([
        publicationsRoute,
        newPublicationRoute,
        publicationRoute,
        subscriptionsRoute,
        subscriptionRoute,
    ]),
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
    interface Register {
        router: typeof router;
    }
}

// An expired session sends the user to the login with the requested route kept for the return.
setUnauthorizedHandler(() => {
    const { pathname, href } = router.state.location;
    if (pathname === "/login" || pathname === "/setup") return;
    void router.navigate({ to: "/login", search: { redirect: safeRedirect(href) } });
});
