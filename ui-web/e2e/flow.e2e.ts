import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { type BrowserContext, expect, type Page, test } from "@playwright/test";
import { messages } from "../src/messages";
import { LIBRARY_FILE_NAME } from "./global-setup";

const CHUNK_BYTES = 8 * 1024 * 1024;
// Two chunks: a full one and a short tail.
const UPLOAD_NAME = "payload.bin";
const UPLOAD = Buffer.alloc(CHUNK_BYTES + 12_345, "axus");
const FIRST_PASSWORD = "correct horse battery";
const SECOND_PASSWORD = "another staple";

const sha256 = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");

let firstUrl: string;
let secondUrl: string;
let first: Page;
let second: Page;
let contexts: BrowserContext[] = [];
let rootHash = "";

// Accepts the next confirmation and resolves with its message. A click that opens a dialog does not return
// until the dialog is closed, so the handler is registered first. If no dialog opens, the promise never
// resolves and the test times out.
function acceptNextDialog(page: Page) {
  return new Promise<string>((resolve) => {
    page.once("dialog", async (dialog) => {
      const message = dialog.message();
      await dialog.accept();
      resolve(message);
    });
  });
}

async function createOwner(page: Page, url: string, password: string) {
  await page.goto(url);
  await expect(page).toHaveURL(/\/setup$/);
  await page.getByLabel(messages.auth.password, { exact: true }).fill(password);
  await page.getByLabel(messages.auth.passwordAgain).fill(password);
  await page.getByRole("button", { name: messages.auth.createOwner }).click();
  await expect(page).toHaveURL(/\/publications$/);
}

test.describe
  .serial("publish on one daemon and download on another", () => {
    test.beforeAll(async ({ browser }) => {
      firstUrl = process.env.AXUS_E2E_FIRST_URL ?? "";
      secondUrl = process.env.AXUS_E2E_SECOND_URL ?? "";
      contexts = [await browser.newContext(), await browser.newContext()];
      first = await contexts[0].newPage();
      second = await contexts[1].newPage();
    });

    test.afterAll(async () => {
      await Promise.all(contexts.map((context) => context.close()));
    });

    test("an unknown client route returns the client", async () => {
      await first.goto(`${firstUrl}/no/such/page`);
      await expect(first.getByText(messages.errors.pageNotFound)).toBeVisible();
    });

    test("the setup refuses mismatched passwords and then creates the owner", async () => {
      await first.goto(firstUrl);
      await expect(first).toHaveURL(/\/setup$/);
      await first.getByLabel(messages.auth.password, { exact: true }).fill(FIRST_PASSWORD);
      await first.getByLabel(messages.auth.passwordAgain).fill("different");
      await first.getByRole("button", { name: messages.auth.createOwner }).click();
      await expect(first.getByRole("alert")).toHaveText(messages.auth.mismatch);

      await first.getByLabel(messages.auth.passwordAgain).fill(FIRST_PASSWORD);
      await first.getByRole("button", { name: messages.auth.createOwner }).click();
      await expect(first).toHaveURL(/\/publications$/);
      await expect(first.getByText(messages.publications.empty)).toBeVisible();
    });

    test("a wrong password is refused and a login returns to the requested route", async () => {
      await first.getByRole("button", { name: messages.nav.logout }).click();
      await expect(first).toHaveURL(/\/login$/);

      await first.goto(`${firstUrl}/subscriptions`);
      await expect(first).toHaveURL(/\/login\?redirect=%2Fsubscriptions$/);
      await first.getByLabel(messages.auth.password, { exact: true }).fill("wrong");
      await first.getByRole("button", { name: messages.auth.login }).click();
      await expect(first.getByRole("alert")).toHaveText(messages.auth.loginFailed);

      await first.getByLabel(messages.auth.password, { exact: true }).fill(FIRST_PASSWORD);
      await first.getByRole("button", { name: messages.auth.login }).click();
      await expect(first).toHaveURL(/\/subscriptions$/);
    });

    test("an upload is sent in 8 MiB chunks and commits", async () => {
      const chunks: string[] = [];
      first.on("request", (request) => {
        if (request.method() === "PUT" && request.url().endsWith("/content")) chunks.push(request.headers()["content-range"] ?? "");
      });
      await first.goto(`${firstUrl}/publications/new`);
      await first
        .getByLabel(messages.publications.upload)
        .setInputFiles({ name: UPLOAD_NAME, mimeType: "application/octet-stream", buffer: UPLOAD });
      await first.getByRole("button", { name: messages.publications.publish }).click();
      await expect(first).toHaveURL(/\/publications\/[^/]+$/);
      await expect(first.getByRole("heading", { name: UPLOAD_NAME })).toBeVisible();
      await expect(first.getByText("Committed", { exact: true })).toBeVisible();

      expect(chunks).toEqual([`bytes 0-${CHUNK_BYTES - 1}/${UPLOAD.length}`, `bytes ${CHUNK_BYTES}-${UPLOAD.length - 1}/${UPLOAD.length}`]);
      const shareText = await first.locator("pre").textContent();
      rootHash = /Root hash: (\S+)/.exec(shareText ?? "")?.[1] ?? "";
      expect(rootHash).toMatch(/^sha3_256:/);
    });

    test("a library file is published from the root without an upload", async () => {
      await first.goto(`${firstUrl}/publications/new`);
      await first.getByLabel(messages.library.chooseRoot).selectOption({ index: 1 });
      await first.getByRole("button", { name: new RegExp(LIBRARY_FILE_NAME) }).click();
      await expect(first).toHaveURL(/\/publications\/[^/]+$/);
      await expect(first.getByRole("heading", { name: LIBRARY_FILE_NAME })).toBeVisible();
      await expect(first.getByText("Committed", { exact: true })).toBeVisible();
    });

    test("the list shows both committed publications", async () => {
      await first.goto(`${firstUrl}/publications`);
      await expect(first.getByRole("link", { name: UPLOAD_NAME })).toBeVisible();
      await expect(first.getByRole("link", { name: LIBRARY_FILE_NAME })).toBeVisible();
    });

    test("the second daemon subscribes by root hash and downloads the same bytes", async () => {
      // The first block arrives about a minute after the subscription, once the daemons have found each other.
      test.setTimeout(300_000);
      await createOwner(second, secondUrl, SECOND_PASSWORD);
      await second.goto(`${secondUrl}/subscriptions`);
      await second.getByLabel(messages.subscriptions.rootHash).fill(rootHash);
      await second.getByLabel(messages.subscriptions.fileName).fill(UPLOAD_NAME);
      await second.getByRole("button", { name: messages.subscriptions.subscribe }).click();
      await expect(second).toHaveURL(/\/subscriptions\/[^/]+$/);

      // The daemons must find each other before the first block arrives.
      await expect(second.getByText("Completed", { exact: true })).toBeVisible({ timeout: 240_000 });
      const [download] = await Promise.all([
        second.waitForEvent("download"),
        second.getByRole("link", { name: messages.subscriptions.download }).click(),
      ]);
      expect(download.suggestedFilename()).toBe(UPLOAD_NAME);
      const saved = await download.path();
      expect(sha256(await readFile(saved))).toBe(sha256(UPLOAD));
    });

    test("a terminal subscription and a committed publication are deleted after a confirmation", async () => {
      const subscriptionConfirm = acceptNextDialog(second);
      await second.getByRole("button", { name: messages.common.delete }).click();
      expect(await subscriptionConfirm).toBe(messages.subscriptions.deleteConfirm);
      await expect(second).toHaveURL(/\/subscriptions$/);
      await expect(second.getByText(messages.subscriptions.empty)).toBeVisible();

      await first.goto(`${firstUrl}/publications`);
      await first.getByRole("link", { name: UPLOAD_NAME }).click();
      const publicationConfirm = acceptNextDialog(first);
      await first.getByRole("button", { name: messages.common.delete }).click();
      expect(await publicationConfirm).toBe(messages.publications.deleteCommitted);
      await expect(first).toHaveURL(/\/publications$/);
      await expect(first.getByRole("link", { name: UPLOAD_NAME })).toHaveCount(0);
      await expect(first.getByRole("link", { name: LIBRARY_FILE_NAME })).toBeVisible();
    });
  });
