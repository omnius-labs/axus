import { defineConfig, devices } from "@playwright/test";

// The flow test starts two daemons from `global-setup.ts` and drives them through the built client.
export default defineConfig({
    testDir: "e2e",
    testMatch: "**/*.e2e.ts",
    globalSetup: "./e2e/global-setup.ts",
    fullyParallel: false,
    workers: 1,
    retries: 0,
    // A transfer between the two daemons needs the node lookup to find the peer first.
    timeout: 180_000,
    expect: { timeout: 15_000 },
    reporter: [["list"]],
    use: { trace: "retain-on-failure" },
    projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
