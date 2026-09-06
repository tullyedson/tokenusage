(async function (fields) {
  async function get(path) {
    const response = await fetch(path, { credentials: "include", cache: "no-store", signal: AbortSignal.timeout(15000) });
    if (!response.ok) throw new Error(response.status === 401 || response.status === 403 ? "Sign in to Claude, then refresh." : "Claude usage request failed (HTTP " + response.status + ").");
    return response.json();
  }
  let organization = fields.organization;
  if (!organization) {
    const organizations = await get("/api/organizations");
    if (!Array.isArray(organizations)) throw new Error("Claude did not return an organization list.");
    const eligible = organizations.filter(item => Array.isArray(item.capabilities) && item.capabilities.includes("chat"));
    const choices = eligible.length ? eligible : organizations;
    if (choices.length !== 1) throw new Error("Set the organization ID in settings to choose which Claude subscription to read.");
    organization = choices[0].uuid;
  }
  if (typeof organization !== "string" || !/^[a-zA-Z0-9-]+$/.test(organization)) throw new Error("The Claude organization ID is invalid.");
  const data = await get("/api/organizations/" + encodeURIComponent(organization) + "/usage");
  const result = {};
  for (const key of ["five_hour", "seven_day", "seven_day_opus", "seven_day_sonnet", "seven_day_oauth_apps", "seven_day_cowork"]) {
    if (data[key]) result[key] = { utilization: data[key].utilization, resets_at: data[key].resets_at };
  }
  if (data.extra_usage) result.extra_usage = { is_enabled: data.extra_usage.is_enabled, monthly_limit: data.extra_usage.monthly_limit, used_credits: data.extra_usage.used_credits };
  return result;
})
