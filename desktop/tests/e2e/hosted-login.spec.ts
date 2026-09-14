import { expect, type Page, test } from "@playwright/test";

import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";
import { seedActiveIdentity } from "../helpers/onboarding";

// Fork: email + 6-digit code login replaces the identity-key onboarding when
// the hosted account service URL is configured. The mock bridge accepts the
// code 123456 and persists the session in localStorage across reloads.

const HOSTED_URL = "https://auth.app.test.invalid";
const HOSTED_SESSION_KEY = "buzz-e2e-hosted-session";
const TRANSACTION_STORAGE_KEY = "buzz-community-onboarding-transaction.v1";
const BLANK_TYLER_IDENTITY = { ...TEST_IDENTITIES.tyler, username: "" };

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
  await expect(page.getByTestId("community-choice-create")).toBeEnabled();
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

test("creating a community from the name hands off to community onboarding", async ({
  page,
}) => {
  await enableHostedAccount(page);
  await installMockBridge(
    page,
    { profileReadError: "no-kind-0" },
    { skipCommunitySeed: true, skipOnboardingSeed: true },
  );
  await page.goto("/");
  await signIn(page);
  await expect(page.getByTestId("hosted-welcome")).toBeVisible();

  await page.getByTestId("community-choice-create").click();
  const nameInput = page.getByTestId("hosted-community-name");
  await expect(nameInput).toBeVisible();

  // Name rules are enforced while typing, with the address previewed.
  await nameInput.fill("My Team");
  await expect(page.getByTestId("hosted-community-preview")).toHaveText(
    "my-team.app.test.invalid",
  );
  await expect(page.getByTestId("hosted-community-feedback")).toHaveText(
    "That address is available.",
  );
  await nameInput.fill("taken");
  await expect(page.getByTestId("hosted-community-feedback")).toHaveText(
    "That address is already taken.",
  );
  await expect(page.getByTestId("hosted-community-submit")).toBeDisabled();
  await nameInput.fill("app");
  await expect(page.getByTestId("hosted-community-feedback")).toHaveText(
    "That name is reserved.",
  );

  await nameInput.fill("my-team");
  await expect(page.getByTestId("hosted-community-feedback")).toHaveText(
    "That address is available.",
  );
  await page.getByTestId("hosted-community-submit").click();

  // The upstream add-community onboarding takes over from here.
  await expect(page.getByTestId("community-onboarding-flow")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(
        (key) => window.localStorage.getItem(key),
        TRANSACTION_STORAGE_KEY,
      ),
    )
    .toContain('"relayUrl":"wss://my-team.app.test.invalid"');
});

test("an account that already owns a community connects to it and cannot create a second", async ({
  page,
}) => {
  await enableHostedAccount(page);
  await page.addInitScript((key) => {
    window.localStorage.setItem(
      key,
      JSON.stringify({
        email: "jin@x.io",
        communityHost: "first.app.test.invalid",
      }),
    );
  }, HOSTED_SESSION_KEY);
  await installMockBridge(
    page,
    { profileReadError: "no-kind-0" },
    { skipCommunitySeed: true, skipOnboardingSeed: true },
  );
  await page.goto("/");

  // A new machine for an account that owns a community connects to it.
  await expect(page.getByTestId("community-onboarding-flow")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(
        (key) => window.localStorage.getItem(key),
        TRANSACTION_STORAGE_KEY,
      ),
    )
    .toContain('"relayUrl":"wss://first.app.test.invalid"');

  // Backing out lands on create-or-join, where a second community is refused.
  await page.getByTestId("community-profile-back").click();
  await expect(page.getByTestId("hosted-welcome")).toBeVisible();
  await expect(page.getByTestId("community-choice-create")).toBeDisabled();
  await expect(page.getByTestId("community-create-limit")).toContainText(
    "first.app.test.invalid",
  );
});

test("hosted mode hides every key surface in settings and routes sign-out", async ({
  page,
}) => {
  await enableHostedAccount(page);
  await page.addInitScript((key) => {
    window.localStorage.setItem(key, JSON.stringify({ email: "jin@x.io" }));
  }, HOSTED_SESSION_KEY);
  // Default seed: a community is configured and onboarding is complete.
  await installMockBridge(page);
  await page.goto("/");

  await page.getByTestId("open-settings").click();
  await page.getByTestId("profile-popover-settings").click();
  await expect(page.getByTestId("settings-nav-profile")).toBeVisible();
  await expect(page.getByTestId("settings-nav-mobile")).toHaveCount(0);
  await expect(page.getByTestId("settings-nav-hosted-communities")).toHaveCount(
    0,
  );

  // Identity details show the public key but no private-key backup row.
  await page.getByTestId("profile-identity-toggle").click();
  await expect(page.getByTestId("profile-pubkey")).toBeVisible();
  await expect(page.getByTestId("profile-private-key-row")).toHaveCount(0);

  // Sign-out asks only for the typed phrase; no nsec is fetched or shown.
  await page.getByTestId("signout-open-dialog").click();
  await expect(page.getByTestId("signout-confirm-phrase")).toBeVisible();
  await expect(page.getByTestId("nsec-value")).toHaveCount(0);
  await expect(page.getByTestId("signout-backup-confirm")).toBeHidden();
  await expect(page.getByTestId("signout-confirm")).toBeDisabled();
  await page.getByTestId("signout-confirm-phrase").fill("wipe all my data");
  await expect(page.getByTestId("signout-confirm")).toBeEnabled();
  await page.getByTestId("signout-confirm").click();
  await expect
    .poll(() =>
      page.evaluate(
        (key) => window.localStorage.getItem(key),
        HOSTED_SESSION_KEY,
      ),
    )
    .toBeNull();
});

test("a stale identity refused by the relay can switch to email sign-in", async ({
  page,
}) => {
  await enableHostedAccount(page);
  await seedActiveIdentity(page, BLANK_TYLER_IDENTITY);
  await page.addInitScript(
    ({ pubkey, transactionKey, sessionKey }) => {
      window.localStorage.setItem(
        `buzz-machine-onboarding-complete.v2:${pubkey}`,
        "true",
      );
      window.localStorage.setItem(
        sessionKey,
        JSON.stringify({ email: "old@x.io" }),
      );
      const timestamp = new Date().toISOString();
      window.localStorage.setItem(
        transactionKey,
        JSON.stringify({
          id: "txn-membership-denied-hosted",
          source: "first-community",
          stage: "profile",
          relayUrl: "wss://denied.example.com",
          communityName: "Denied",
          communityId: "e2e-default-community",
          createdAt: timestamp,
          updatedAt: timestamp,
        }),
      );
    },
    {
      pubkey: BLANK_TYLER_IDENTITY.pubkey,
      transactionKey: TRANSACTION_STORAGE_KEY,
      sessionKey: HOSTED_SESSION_KEY,
    },
  );
  await installMockBridge(
    page,
    {
      profileUpdateError:
        "relay returned 403 Forbidden: You must be a relay member to access this relay",
    },
    { relayWsUrl: "wss://denied.example.com", skipOnboardingSeed: true },
  );
  await page.goto("/");

  await page.getByTestId("community-profile-name-key").fill("Kalvin");
  await page.getByTestId("community-profile-next").click();
  await expect(page.getByTestId("membership-denied")).toBeVisible();

  // Hosted mode offers email sign-in and hides the raw-key import.
  await expect(page.getByTestId("membership-denied-change-key")).toHaveCount(0);
  await page.getByTestId("membership-denied-hosted-sign-in").click();
  await expect
    .poll(() =>
      page.evaluate(
        (key) => window.localStorage.getItem(key),
        HOSTED_SESSION_KEY,
      ),
    )
    .toBeNull();
});
