(async function (fields) {
  const authResponse = await fetch("/api/auth/session", { credentials: "include", cache: "no-store", signal: AbortSignal.timeout(15000) });
  if (!authResponse.ok) throw new Error("Sign in to ChatGPT, then refresh.");
  const auth = await authResponse.json();
  if (!auth.accessToken) throw new Error("Sign in to ChatGPT, then refresh.");
  const headers = { Authorization: "Bearer " + auth.accessToken };
  if (fields.account_id) headers["ChatGPT-Account-Id"] = fields.account_id;
  const response = await fetch("/backend-api/wham/usage", { headers, credentials: "include", cache: "no-store", signal: AbortSignal.timeout(15000) });
  if (!response.ok) throw new Error("OpenAI could not read the Codex allowance (HTTP " + response.status + "). Try the signed-in Codex connection in settings.");
  const data = await response.json();
  return { rate_limit: data.rate_limit, additional_rate_limits: data.additional_rate_limits, credits: data.credits, plan_type: data.plan_type };
})
