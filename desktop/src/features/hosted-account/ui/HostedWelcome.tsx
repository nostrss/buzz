import * as React from "react";

import {
  type HostedMe,
  hostedLogout,
  hostedSessionMe,
} from "@/features/hosted-account/api";
import { HostedCommunityCreate } from "@/features/hosted-account/ui/HostedCommunityCreate";
import { useCommunityOnboarding } from "@/features/onboarding/communityOnboarding";
import { InviteRedeemForm } from "@/features/onboarding/ui/InviteRedeemForm";
import { hostedCommunityDomain } from "@/shared/config/hostedAccount";
import { useSystemColorScheme } from "@/shared/theme/useSystemColorScheme";
import { Card } from "@/shared/ui/card";
import { StartupWindowDragRegion } from "@/shared/ui/StartupWindowDragRegion";

const CARD_CLASS =
  "w-full max-w-[320px] items-center px-6 py-4 text-center text-sm font-normal leading-6 text-foreground [--buzz-card-textured-min-height:88px] transition-[filter] duration-150 ease-out hover:brightness-[0.98] focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-foreground/35 disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:brightness-100";

type HostedWelcomeProps = {
  accountUrl: string;
};

/**
 * Fork-only first screen after email login when this machine has no
 * community yet: create one (one per account), or paste an invite link.
 */
export function HostedWelcome({ accountUrl }: HostedWelcomeProps) {
  const systemColorScheme = useSystemColorScheme();
  const communityOnboarding = useCommunityOnboarding();
  const [page, setPage] = React.useState<"choose" | "create" | "join">(
    "choose",
  );
  const [me, setMe] = React.useState<HostedMe | null | undefined>(undefined);
  const [signingOut, setSigningOut] = React.useState(false);
  const [signOutError, setSignOutError] = React.useState<string | null>(null);

  React.useEffect(() => {
    let cancelled = false;
    void hostedSessionMe(accountUrl)
      .then((result) => {
        if (!cancelled) setMe(result);
      })
      .catch(() => {
        if (!cancelled) setMe(null);
      });
    return () => {
      cancelled = true;
    };
  }, [accountUrl]);

  const canCreate =
    me === undefined ? false : (me?.can_create_community ?? true);
  const ownedHost = me?.communities[0]?.host ?? null;

  const startConnection = React.useCallback(
    (relayUrl: string) => {
      communityOnboarding.start({
        source: "first-community",
        firstCommunityPage: "join",
        relayUrl,
      });
    },
    [communityOnboarding],
  );

  const redeemInvite = React.useCallback(
    (relayUrl: string, code: string, policyReceipt?: string) => {
      communityOnboarding.start({
        source: "first-community",
        firstCommunityPage: "join",
        relayUrl,
        inviteCode: code,
        policyReceipt,
      });
    },
    [communityOnboarding],
  );

  const signOut = React.useCallback(async () => {
    setSigningOut(true);
    setSignOutError(null);
    try {
      await hostedLogout(accountUrl);
    } catch (caught) {
      setSignOutError(
        caught instanceof Error ? caught.message : String(caught),
      );
      setSigningOut(false);
    }
  }, [accountUrl]);

  return (
    <div
      className="buzz-onboarding-neutral-theme buzz-startup-shell flex h-dvh items-start justify-center overflow-y-auto bg-background px-4 pb-24 pt-[106px] text-foreground"
      data-system-color-scheme={systemColorScheme}
      data-testid="hosted-welcome"
    >
      <StartupWindowDragRegion />
      <div className="relative flex min-h-0 w-full max-w-[920px] flex-1 flex-col items-center text-center">
        {page === "choose" ? (
          <>
            <div className="w-full max-w-[760px]">
              <h1 className="text-title font-normal">
                Create or join a community
              </h1>
              <p className="mt-3 text-sm leading-6 text-foreground/80">
                Start a community for your team, or join one with an invite
                link.
              </p>
            </div>
            <div className="flex w-full flex-col items-center justify-center gap-12 py-16">
              <div className="flex w-full flex-col items-center gap-2">
                <Card asChild className={CARD_CLASS} variant="textured">
                  <button
                    data-testid="community-choice-create"
                    disabled={!canCreate}
                    onClick={() => setPage("create")}
                    type="button"
                  >
                    Create a community
                  </button>
                </Card>
                {me !== undefined && !canCreate ? (
                  <p
                    className="max-w-[320px] text-xs leading-5 text-muted-foreground"
                    data-testid="community-create-limit"
                  >
                    Each account can create one community
                    {ownedHost ? ` — yours is ${ownedHost}` : ""}.
                  </p>
                ) : null}
              </div>
              <Card asChild className={CARD_CLASS} variant="textured">
                <button
                  data-testid="community-choice-join"
                  onClick={() => setPage("join")}
                  type="button"
                >
                  Join with an invite link
                </button>
              </Card>
            </div>
            <div className="relative z-10 flex flex-col items-center gap-2 text-xs text-muted-foreground">
              {signOutError ? (
                <p className="text-destructive" role="alert">
                  {signOutError}
                </p>
              ) : null}
              <button
                className="underline-offset-2 hover:underline"
                data-testid="hosted-welcome-sign-out"
                disabled={signingOut}
                onClick={() => void signOut()}
                type="button"
              >
                {signingOut ? "Signing out…" : "Sign out"}
              </button>
            </div>
          </>
        ) : page === "create" ? (
          <>
            <div className="w-full max-w-[760px]">
              <h1 className="text-title font-normal">Name your community</h1>
              <p className="mt-3 text-sm leading-6 text-foreground/80">
                The name becomes your community’s address.
              </p>
            </div>
            <div className="flex w-full flex-col items-center py-12">
              <HostedCommunityCreate
                accountUrl={accountUrl}
                communityDomain={hostedCommunityDomain()}
                onCancel={() => setPage("choose")}
                onComplete={() => setPage("choose")}
              />
            </div>
          </>
        ) : (
          <>
            <div className="w-full max-w-[760px]">
              <h1 className="text-title font-normal">Join a community</h1>
              <p className="mt-3 text-sm leading-6 text-foreground/80">
                Paste the invite link you received.
              </p>
            </div>
            <div className="flex w-full flex-1 flex-col items-center justify-center gap-16">
              <InviteRedeemForm
                error={null}
                isRedeeming={false}
                onCancel={() => setPage("choose")}
                onConnect={startConnection}
                onRedeem={redeemInvite}
                placeholder="Invite link or community URL"
                variant="onboarding-spotlight"
              />
            </div>
          </>
        )}
      </div>
    </div>
  );
}
