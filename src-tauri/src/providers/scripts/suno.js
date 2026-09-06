(async function () {
  for (let attempt = 0; attempt < 40; attempt++) {
    if (window.Clerk && (window.Clerk.session || window.Clerk.loaded)) break;
    await new Promise(resolve => setTimeout(resolve, 200));
  }
  const session = window.Clerk && window.Clerk.session;
  if (!session) throw new Error("Sign in to Suno, then refresh.");
  const token = await session.getToken();
  if (!token) throw new Error("Suno sign-in has expired. Sign in again.");
  const response = await fetch("https://studio-api.prod.suno.com/api/billing/info/", {
    headers: { Authorization: "Bearer " + token },
    cache: "no-store", signal: AbortSignal.timeout(15000)
  });
  if (!response.ok) throw new Error("Suno could not read credits (HTTP " + response.status + "). Sign in again if needed.");
  const data = await response.json();
  return { total_credits_left: data.total_credits_left, monthly_limit: data.monthly_limit, monthly_usage: data.monthly_usage, period_end: data.period_end, current_period_end: data.current_period_end };
})
