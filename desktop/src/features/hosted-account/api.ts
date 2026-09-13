import { invokeTauri } from "@/shared/api/tauri";

/** Structured error body from the account service (`{"error": code, ...}`). */
export type HostedAccountError = {
  error: string;
  remaining_attempts?: number;
  retry_after_seconds?: number;
};

export type HostedCommunitySummary = { host: string; role: string };

export type HostedLoginResult = {
  pubkey: string;
  communities: HostedCommunitySummary[];
};

export type HostedMe = {
  email: string;
  pubkey: string;
  communities: HostedCommunitySummary[];
  can_create_community: boolean;
};

export function isHostedAccountError(
  value: unknown,
): value is HostedAccountError {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { error?: unknown }).error === "string"
  );
}

export async function hostedLoginStart(
  baseUrl: string,
  email: string,
): Promise<HostedAccountError | { status: "sent" }> {
  return invokeTauri("hosted_login_start", { baseUrl, email });
}

/** Exchanges the code; on success the identity key is already in the keychain. */
export async function hostedLoginVerify(
  baseUrl: string,
  email: string,
  code: string,
): Promise<HostedAccountError | HostedLoginResult> {
  return invokeTauri("hosted_login_verify", { baseUrl, email, code });
}

/** `null` when there is no stored session or the server rejected it. */
export async function hostedSessionMe(
  baseUrl: string,
): Promise<HostedMe | null> {
  return invokeTauri("hosted_session_me", { baseUrl });
}

/** Revokes the session and wipes local state; the app relaunches. */
export async function hostedLogout(baseUrl: string): Promise<void> {
  await invokeTauri("hosted_logout", { baseUrl });
}

export type HostedCommunityCheck = {
  name: string;
  normalized: string;
  host: string;
  available: boolean;
  reason?: string;
};

export type HostedCommunityCreated = {
  host: string;
  relay_url: string;
  community_id: string;
};

export async function hostedCommunityCheck(
  baseUrl: string,
  name: string,
): Promise<HostedAccountError | HostedCommunityCheck> {
  return invokeTauri("hosted_community_check", { baseUrl, name });
}

export async function hostedCommunityCreate(
  baseUrl: string,
  name: string,
): Promise<HostedAccountError | HostedCommunityCreated> {
  return invokeTauri("hosted_community_create", { baseUrl, name });
}

/**
 * Mirror of the account service's name rules so the form can validate while
 * typing; the service remains the authority.
 */
export const COMMUNITY_NAME_RESERVED = [
  "app",
  "auth",
  "www",
  "admin",
  "api",
  "mail",
  "relay",
];

export function normalizeCommunityName(raw: string): string {
  return raw.trim().toLowerCase().replace(/\s+/g, "-").replace(/-+/g, "-");
}

/** Returns the rejection reason, or null when the normalized name is valid. */
export function communityNameProblem(normalized: string): string | null {
  if (normalized.length < 3) return "too_short";
  if (normalized.length > 30) return "too_long";
  if (!/^[a-z0-9-]+$/.test(normalized)) return "invalid_characters";
  if (normalized.startsWith("-") || normalized.endsWith("-"))
    return "leading_or_trailing_hyphen";
  if (COMMUNITY_NAME_RESERVED.includes(normalized)) return "reserved";
  return null;
}

export function describeCommunityNameProblem(reason: string): string {
  switch (reason) {
    case "too_short":
      return "Use at least 3 characters.";
    case "too_long":
      return "Use at most 30 characters.";
    case "invalid_characters":
      return "Use lowercase letters, numbers, and hyphens only.";
    case "leading_or_trailing_hyphen":
      return "Names can’t start or end with a hyphen.";
    case "reserved":
      return "That name is reserved.";
    case "taken":
      return "That address is already taken.";
    default:
      return "That name can’t be used.";
  }
}

/** Human-readable message for a known account-service error code. */
export function describeHostedAccountError(error: HostedAccountError): string {
  switch (error.error) {
    case "invalid_email":
      return "Enter a valid email address.";
    case "code_recently_sent":
      return `A code was sent recently. Try again in ${error.retry_after_seconds ?? 60}s.`;
    case "mail_send_failed":
      return "We couldn't send the email. Try again in a moment.";
    case "invalid_code_format":
      return "Enter the 6-digit code from the email.";
    case "invalid_code":
      return error.remaining_attempts && error.remaining_attempts > 0
        ? `That code isn't right. ${error.remaining_attempts} ${error.remaining_attempts === 1 ? "try" : "tries"} left.`
        : "Too many wrong codes. Request a new one.";
    case "code_expired":
      return "That code expired. Request a new one.";
    case "no_active_code":
      return "No active code for this email. Request a new one.";
    case "missing_token":
    case "invalid_token":
      return "Your session ended. Sign in again.";
    case "community_limit_reached":
      return "Each account can create one community.";
    case "host_taken":
      return "That address was just taken. Try another name.";
    case "invalid_name":
      return "That name can’t be used.";
    case "relay_error":
      return "The community service is unavailable. Try again in a moment.";
    default:
      return "Something went wrong. Try again.";
  }
}
