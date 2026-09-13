import { AlertCircle, LoaderCircle } from "lucide-react";
import * as React from "react";

import {
  CHANNEL_FORM_FIELD_CONTROL_CLASS,
  CHANNEL_FORM_FIELD_SHELL_CLASS,
} from "@/features/channels/ui/channelFormStyles";
import {
  communityNameProblem,
  describeCommunityNameProblem,
  describeHostedAccountError,
  hostedCommunityCheck,
  hostedCommunityCreate,
  isHostedAccountError,
  normalizeCommunityName,
} from "@/features/hosted-account/api";
import { useCommunityOnboarding } from "@/features/onboarding/communityOnboarding";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";

type HostedCommunityCreateProps = {
  accountUrl: string;
  /** Domain suffix shown in the address preview, e.g. `app.pegboard.me`. */
  communityDomain: string | null;
  onCancel?: () => void;
  onComplete: () => void;
};

/**
 * Fork-only: one field. The name becomes the community address
 * (`<name>.<domain>`); availability is checked while typing and creation
 * hands off to the upstream add-community onboarding.
 */
export function HostedCommunityCreate({
  accountUrl,
  communityDomain,
  onCancel,
  onComplete,
}: HostedCommunityCreateProps) {
  const onboarding = useCommunityOnboarding();
  const [name, setName] = React.useState("");
  const [availability, setAvailability] = React.useState<{
    normalized: string;
    available: boolean;
    reason?: string;
  } | null>(null);
  const [checking, setChecking] = React.useState(false);
  const [creating, setCreating] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const normalized = normalizeCommunityName(name);
  const problem = normalized ? communityNameProblem(normalized) : null;
  const preview = normalized
    ? `${normalized}.${communityDomain ?? "…"}`
    : `your-team.${communityDomain ?? "…"}`;

  React.useEffect(() => {
    if (!normalized || problem) {
      setChecking(false);
      setAvailability(null);
      return;
    }
    let cancelled = false;
    setChecking(true);
    const handle = window.setTimeout(() => {
      void hostedCommunityCheck(accountUrl, normalized)
        .then((result) => {
          if (cancelled) return;
          if (isHostedAccountError(result)) {
            setAvailability(null);
            setError(describeHostedAccountError(result));
            return;
          }
          setAvailability({
            normalized: result.normalized,
            available: result.available,
            reason: result.reason,
          });
        })
        .catch(() => {
          if (!cancelled) setAvailability(null);
        })
        .finally(() => {
          if (!cancelled) setChecking(false);
        });
    }, 400);
    return () => {
      cancelled = true;
      window.clearTimeout(handle);
    };
  }, [accountUrl, normalized, problem]);

  const current =
    availability && availability.normalized === normalized
      ? availability
      : null;
  const feedback = problem
    ? describeCommunityNameProblem(problem)
    : checking
      ? "Checking availability…"
      : current
        ? current.available
          ? "That address is available."
          : describeCommunityNameProblem(current.reason ?? "taken")
        : "You can’t change the address after creating the community.";
  const feedbackIsError = Boolean(problem) || current?.available === false;
  const canSubmit =
    Boolean(normalized) &&
    !problem &&
    !checking &&
    !creating &&
    current?.available !== false;

  const create = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!canSubmit) return;
    setCreating(true);
    setError(null);
    try {
      const result = await hostedCommunityCreate(accountUrl, normalized);
      if (isHostedAccountError(result)) {
        if (result.error === "host_taken") {
          setAvailability({ normalized, available: false, reason: "taken" });
        }
        setError(describeHostedAccountError(result));
        return;
      }
      const started = onboarding.start({
        source: "add-community",
        relayUrl: result.relay_url,
        communityName: normalized,
      });
      if (!started) {
        setError(
          "Finish connecting the community already in progress, then try again.",
        );
        return;
      }
      onComplete();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setCreating(false);
    }
  };

  return (
    <form
      className="w-full max-w-[420px] space-y-5 text-left"
      data-testid="hosted-community-create"
      onSubmit={(event) => void create(event)}
    >
      <div className="space-y-1.5">
        <label
          className="text-sm font-medium text-foreground"
          htmlFor="hosted-community-name"
        >
          Community name
        </label>
        <div
          className={cn(
            "flex min-h-11 items-center px-3",
            CHANNEL_FORM_FIELD_SHELL_CLASS,
          )}
        >
          <Input
            autoCapitalize="none"
            autoComplete="off"
            autoCorrect="off"
            autoFocus
            className={cn(
              "h-8 min-w-0 px-0 py-0 leading-6",
              CHANNEL_FORM_FIELD_CONTROL_CLASS,
            )}
            data-testid="hosted-community-name"
            disabled={creating}
            id="hosted-community-name"
            maxLength={40}
            onChange={(event) => {
              setName(event.target.value);
              setError(null);
            }}
            placeholder="your-team"
            spellCheck={false}
            value={name}
          />
        </div>
        <p
          className="break-all font-mono text-xs text-muted-foreground"
          data-testid="hosted-community-preview"
        >
          {preview}
        </p>
        <p
          className={cn(
            "text-xs leading-5",
            feedbackIsError ? "text-destructive" : "text-muted-foreground",
          )}
          data-testid="hosted-community-feedback"
        >
          {feedback}
        </p>
      </div>
      {error ? (
        <div
          className="flex items-start gap-3 rounded-xl border border-destructive/30 bg-destructive/10 p-4"
          data-testid="hosted-community-error"
          role="alert"
        >
          <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-destructive" />
          <p className="text-sm leading-5 text-destructive">{error}</p>
        </div>
      ) : null}
      <div className="flex justify-end gap-2 pt-1">
        {onCancel ? (
          <Button
            disabled={creating}
            onClick={onCancel}
            type="button"
            variant="outline"
          >
            Back
          </Button>
        ) : null}
        <Button
          data-testid="hosted-community-submit"
          disabled={!canSubmit}
          type="submit"
        >
          {creating ? <LoaderCircle className="h-4 w-4 animate-spin" /> : null}
          {creating ? "Creating…" : "Create community"}
        </Button>
      </div>
    </form>
  );
}
