import type { QueryClient } from "@tanstack/react-query";
import * as React from "react";

import {
  describeHostedAccountError,
  type HostedCommunitySummary,
  hostedLoginStart,
  hostedLoginVerify,
  hostedSessionMe,
  isHostedAccountError,
} from "@/features/hosted-account/api";
import { useCommunityOnboarding } from "@/features/onboarding/communityOnboarding";
import { useIdentityQuery } from "@/shared/api/hooks";
import { useSystemColorScheme } from "@/shared/theme/useSystemColorScheme";
import { Button } from "@/shared/ui/button";
import { FlappingBee } from "@/shared/ui/buzz-logo/FlappingBee";
import { Input } from "@/shared/ui/input";
import { StartupWindowDragRegion } from "@/shared/ui/StartupWindowDragRegion";

type Phase = "checking" | "email" | "code";

type HostedLoginFlowProps = {
  accountUrl: string;
  /** Marks machine onboarding complete for the signed-in identity. */
  complete: (pubkey: string) => void;
  /** Pins onboarding to the imported identity until `complete` settles it. */
  continueWithIdentity: (pubkey: string) => void;
  /** Whether this machine already has a community configured. */
  hasCommunity: boolean;
  queryClient: QueryClient;
};

/**
 * Fork-only replacement for the identity-key onboarding: email in, 6-digit
 * code in, done. The key is issued by the account service and committed to
 * the keychain inside the Tauri command, so this screen never sees it.
 */
export function HostedLoginFlow({
  accountUrl,
  complete,
  continueWithIdentity,
  hasCommunity,
  queryClient,
}: HostedLoginFlowProps) {
  const systemColorScheme = useSystemColorScheme();
  const identityQuery = useIdentityQuery();
  const communityOnboarding = useCommunityOnboarding();
  const [phase, setPhase] = React.useState<Phase>("checking");
  const [email, setEmail] = React.useState("");
  const [code, setCode] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [needsNewCode, setNeedsNewCode] = React.useState(false);

  const finish = React.useCallback(
    async (pubkey: string, communities: HostedCommunitySummary[]) => {
      continueWithIdentity(pubkey);
      await queryClient.invalidateQueries({ queryKey: ["identity"] });
      // A new machine for an account that already owns a community: connect
      // to it through the upstream add-community flow instead of showing the
      // create-or-join screen. Invite deep links already in flight win.
      const first = communities[0];
      if (first && !hasCommunity && !communityOnboarding.transaction) {
        communityOnboarding.start({
          source: "add-community",
          relayUrl: `wss://${first.host}`,
          communityName: first.host.split(".")[0] ?? first.host,
        });
      }
      complete(pubkey);
    },
    [
      communityOnboarding.start,
      communityOnboarding.transaction,
      complete,
      continueWithIdentity,
      hasCommunity,
      queryClient,
    ],
  );

  // A stored session whose identity is the one in the keychain skips login.
  const identityPubkey = identityQuery.data?.pubkey;
  const identitySettled = identityQuery.status !== "pending";
  React.useEffect(() => {
    if (phase !== "checking" || !identitySettled) return;
    let cancelled = false;
    void hostedSessionMe(accountUrl)
      .then((me) => {
        if (cancelled) return;
        if (me && me.pubkey === identityPubkey) {
          void finish(me.pubkey, me.communities);
        } else {
          setPhase("email");
        }
      })
      .catch(() => {
        if (!cancelled) setPhase("email");
      });
    return () => {
      cancelled = true;
    };
  }, [accountUrl, finish, identityPubkey, identitySettled, phase]);

  const sendCode = React.useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await hostedLoginStart(accountUrl, email);
      if (isHostedAccountError(result)) {
        setError(describeHostedAccountError(result));
        return;
      }
      setCode("");
      setNeedsNewCode(false);
      setPhase("code");
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }, [accountUrl, email]);

  const verifyCode = React.useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await hostedLoginVerify(accountUrl, email, code);
      if (isHostedAccountError(result)) {
        setError(describeHostedAccountError(result));
        setNeedsNewCode(
          result.error === "code_expired" ||
            result.error === "no_active_code" ||
            (result.error === "invalid_code" &&
              !(result.remaining_attempts && result.remaining_attempts > 0)),
        );
        return;
      }
      await finish(result.pubkey, result.communities);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }, [accountUrl, code, email, finish]);

  return (
    <div
      className="buzz-onboarding-neutral-theme buzz-startup-shell fixed inset-0 z-40 flex items-center justify-center bg-background px-4 py-8 text-foreground"
      data-system-color-scheme={systemColorScheme}
      data-testid="hosted-login"
    >
      <StartupWindowDragRegion />
      <div className="relative flex w-full max-w-[420px] flex-col items-center text-center">
        <FlappingBee className="h-auto w-24" />
        {phase === "checking" ? (
          <p className="mt-6 text-sm text-muted-foreground">Signing you in…</p>
        ) : phase === "email" ? (
          <form
            className="mt-6 flex w-full flex-col items-center gap-4"
            onSubmit={(event) => {
              event.preventDefault();
              void sendCode();
            }}
          >
            <h1 className="text-3xl font-semibold tracking-tight">
              Sign in with your email
            </h1>
            <p className="text-sm leading-6 text-muted-foreground">
              We’ll email you a 6-digit code. No password needed.
            </p>
            <Input
              aria-label="Email address"
              autoComplete="email"
              autoFocus
              className="h-11 w-full text-center"
              data-testid="hosted-login-email"
              disabled={busy}
              inputMode="email"
              onChange={(event) => setEmail(event.target.value)}
              placeholder="you@company.com"
              type="email"
              value={email}
            />
            {error ? (
              <p
                className="text-sm text-destructive"
                data-testid="hosted-login-error"
                role="alert"
              >
                {error}
              </p>
            ) : null}
            <Button
              className="h-10 w-full"
              data-testid="hosted-login-send"
              disabled={busy || !email.trim()}
              type="submit"
            >
              {busy ? "Sending…" : "Send code"}
            </Button>
          </form>
        ) : (
          <form
            className="mt-6 flex w-full flex-col items-center gap-4"
            onSubmit={(event) => {
              event.preventDefault();
              void verifyCode();
            }}
          >
            <h1 className="text-3xl font-semibold tracking-tight">
              Check your email
            </h1>
            <p className="text-sm leading-6 text-muted-foreground">
              Enter the 6-digit code we sent to{" "}
              <span className="font-medium text-foreground">{email}</span>.
            </p>
            <Input
              aria-label="6-digit code"
              autoComplete="one-time-code"
              autoFocus
              className="h-11 w-full text-center text-lg tracking-[0.4em]"
              data-testid="hosted-login-code"
              disabled={busy || needsNewCode}
              inputMode="numeric"
              maxLength={6}
              onChange={(event) =>
                setCode(event.target.value.replace(/\D/g, "").slice(0, 6))
              }
              pattern="[0-9]{6}"
              placeholder="123456"
              value={code}
            />
            {error ? (
              <p
                className="text-sm text-destructive"
                data-testid="hosted-login-error"
                role="alert"
              >
                {error}
              </p>
            ) : null}
            {needsNewCode ? (
              <Button
                className="h-10 w-full"
                data-testid="hosted-login-resend"
                disabled={busy}
                onClick={() => void sendCode()}
                type="button"
              >
                Send a new code
              </Button>
            ) : (
              <Button
                className="h-10 w-full"
                data-testid="hosted-login-verify"
                disabled={busy || code.length !== 6}
                type="submit"
              >
                {busy ? "Signing in…" : "Continue"}
              </Button>
            )}
            <div className="flex gap-4 text-xs text-muted-foreground">
              <button
                className="underline-offset-2 hover:underline"
                data-testid="hosted-login-change-email"
                disabled={busy}
                onClick={() => {
                  setError(null);
                  setNeedsNewCode(false);
                  setPhase("email");
                }}
                type="button"
              >
                Use a different email
              </button>
              {!needsNewCode ? (
                <button
                  className="underline-offset-2 hover:underline"
                  data-testid="hosted-login-resend-link"
                  disabled={busy}
                  onClick={() => void sendCode()}
                  type="button"
                >
                  Resend code
                </button>
              ) : null}
            </div>
          </form>
        )}
      </div>
    </div>
  );
}
