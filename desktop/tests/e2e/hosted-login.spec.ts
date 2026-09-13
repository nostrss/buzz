import { expect, type Page, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

// Fork: email + 6-digit code login replaces the identity-key onboarding when
// the hosted account service URL is configured. The mock bridge accepts the
// code 123456 and persists the session in localStorage across reloads.

const HOSTED_URL = "https://auth.app.test.invalid";
const HOSTED_SESSION_KEY = "buzz-e2e-hosted-session";
const TRANSACTION_STORAGE_KEY = "buzz-community-onboarding-transaction.v1";

async function enableHostedAccount(page: Page) {
  await page.addInitScript((url) => {
    (
      window as Window & { __BUZZ_E2E_HOSTED_ACCOUNT_URL__?: string }
    ).__BUZZ_E2E_HOSTED_ACCOUNT_URL__ = url;
  }, HOSTED_URL);
}

async function signIn(page: Page, email = "jin@example.com") {
  await page.getByTestId("hosted-login-email").fill(email);
  await page.getByTestId("hosted-login-send").click();
  await expect(page.getByTestId("hosted-login-code")).toBeVisible();
  await page.getByTestId("hosted-login-code").fill("123456");
  await page.getByTestId("hosted-login-verify").click();
}

test("email and code sign-in lands on create-or-join with no key screens", async ({
  page,
}) => {
  await enableHostedAccount(page);
  await installMockBridge(page, undefined, {
    skipCommunitySeed: true,
    skipOnboardingSeed: true,
  });
  await page.goto("/");

  const login = page.getByTestId("hosted-login");
  await expect(login).toBeVisible();
  await expect(page.getByTestId("machine-onboarding-gate")).toHaveCount(0);
  await expect(page.getByText(/identity key/i)).toHaveCount(0);
  await expect(page.getByText(/nsec/i)).toHaveCount(0);

  // Wrong code reports the remaining tries and stays on the code step.
  await page.getByTestId("hosted-login-email").fill("jin@example.com");
  await page.getByTestId("hosted-login-send").click();
  await expect(page.getByTestId("hosted-login-code")).toBeVisible();
  await page.getByTestId("hosted-login-code").fill("000000");
  await page.getByTestId("hosted-login-verify").click();
  await expect(page.getByTestId("hosted-login-error")).toContainText(
    "4 tries left",
  );

  await page.getByTestId("hosted-login-code").fill("123456");
  await page.getByTestId("hosted-login-verify").click();

  const welcome = page.getByTestId("hosted-welcome");
  await expect(welcome).toBeVisible();
  await expect(page.getByTestId("community-choice-create")).toBeDisabled();
  await expect(page.getByTestId("community-choice-join")).toBeEnabled();
  await expect(page.getByTestId("community-choice-existing")).toHaveCount(0);
  await expect(page.getByText(/identity key/i)).toHaveCount(0);

  // The join card opens the upstream invite form.
  await page.getByTestId("community-choice-join").click();
  await expect(
    page.getByPlaceholder("Invite link or community URL"),
  ).toBeVisible();
});

test("a stored session skips the login screens on the next launch", async ({
  page,
}) => {
  await enableHostedAccount(page);
  await page.addInitScript((key) => {
    window.localStorage.setItem(key, JSON.stringify({ email: "jin@x.io" }));
  }, HOSTED_SESSION_KEY);
  await installMockBridge(page, undefined, {
    skipCommunitySeed: true,
    skipOnboardingSeed: true,
  });
  await page.goto("/");

  await expect(page.getByTestId("hosted-welcome")).toBeVisible();
  await expect(page.getByTestId("hosted-login-email")).toHaveCount(0);
});

test("sign out forgets the session", async ({ page }) => {
  await enableHostedAccount(page);
  await installMockBridge(page, undefined, {
    skipCommunitySeed: true,
    skipOnboardingSeed: true,
  });
  await page.goto("/");
  await signIn(page);
  await expect(page.getByTestId("hosted-welcome")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(
        (key) => window.localStorage.getItem(key),
        HOSTED_SESSION_KEY,
      ),
    )
    .not.toBeNull();

  await page.getByTestId("hosted-welcome-sign-out").click();
  await expect
    .poll(() =>
      page.evaluate(
        (key) => window.localStorage.getItem(key),
        HOSTED_SESSION_KEY,
      ),
    )
    .toBeNull();
});

test("an invite deep link waits for sign-in, then claims", async ({ page }) => {
  await enableHostedAccount(page);
  await page.route("**/api/invites/claim", async (route) => {
    await route.abort();
  });
  await installMockBridge(
    page,
    {
      pendingCommunityDeepLinks: [
        {
          id: "dl-join-hosted",
          kind: "join" as const,
          relayUrl: "wss://hive.example.com",
          code: "abc.def",
        },
      ],
    },
    { skipCommunitySeed: true, skipOnboardingSeed: true },
  );
  await page.goto("/");

  const gate = page.getByTestId("pending-invite-gate");
  await expect(gate).toBeVisible();
  await page.getByTestId("pending-invite-continue").click();
  await expect(gate).toHaveCount(0);
  await expect(page.getByTestId("hosted-login")).toBeVisible();

  await signIn(page);

  // After sign-in the persisted transaction resumes at claiming instead of
  // showing the create-or-join screen.
  await expect(page.getByTestId("hosted-welcome")).toHaveCount(0);
  await expect
    .poll(() =>
      page.evaluate(
        (key) => window.localStorage.getItem(key),
        TRANSACTION_STORAGE_KEY,
      ),
    )
    .toContain('"stage":"claiming"');
});

test("without the account service URL the upstream onboarding is unchanged", async ({
  page,
}) => {
  await installMockBridge(page, undefined, {
    skipCommunitySeed: true,
    skipOnboardingSeed: true,
  });
  await page.goto("/");

  await expect(page.getByTestId("machine-onboarding-gate")).toBeVisible();
  await expect(page.getByTestId("hosted-login")).toHaveCount(0);
  await expect(page.getByTestId("identity-key-help-trigger")).toBeVisible();
});
