/**
 * Fork-only: the hosted-service account service base URL.
 *
 * Set `VITE_BUZZ_ACCOUNT_URL` at build time to switch the app from upstream's
 * identity-key onboarding to email + code login backed by `buzz-account`.
 * Empty (the upstream default) leaves every upstream flow untouched. E2E specs
 * override it per page through `window.__BUZZ_E2E_HOSTED_ACCOUNT_URL__`.
 */
export function hostedAccountUrl(): string | null {
  if (typeof window !== "undefined") {
    const override = (
      window as Window & { __BUZZ_E2E_HOSTED_ACCOUNT_URL__?: string }
    ).__BUZZ_E2E_HOSTED_ACCOUNT_URL__;
    if (override !== undefined) return override.trim() || null;
  }
  const configured = import.meta.env?.VITE_BUZZ_ACCOUNT_URL;
  return typeof configured === "string" && configured.trim()
    ? configured.trim().replace(/\/+$/, "")
    : null;
}

/**
 * Domain communities are created under, derived from the account service
 * host by convention (`auth.<domain>` → `<domain>`).
 */
export function hostedCommunityDomain(): string | null {
  const url = hostedAccountUrl();
  if (!url) return null;
  try {
    return new URL(url).hostname.replace(/^auth\./, "");
  } catch {
    return null;
  }
}
