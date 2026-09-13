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
    default:
      return "Something went wrong. Try again.";
  }
}
