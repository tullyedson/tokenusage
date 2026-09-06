(async function () {
  for (let attempt = 0; attempt < 40; attempt++) {
    if (window.Clerk && (window.Clerk.session || window.Clerk.loaded)) break;
    await new Promise(resolve => setTimeout(resolve, 200));
  }
  const session = window.Clerk && window.Clerk.session;
  if (!session) throw new Error("Sign in to Higgsfield, then refresh.");
  const token = await session.getToken();
  if (!token) throw new Error("Higgsfield sign-in has expired. Sign in again.");
  const response = await fetch("https://fnf-api-gw.higgsfield.ai/fnf/workspaces/wallet", {
    headers: { Authorization: "Bearer " + token },
    cache: "no-store", signal: AbortSignal.timeout(15000)
  });
  if (!response.ok) throw new Error("Higgsfield could not read the wallet (HTTP " + response.status + "). Open its account page and check the selected workspace.");
  const data = await response.json();
  return { subscription_balance: data.subscription_balance, total_credits: data.total_credits, credits_balance: data.credits_balance, on_demand_credits: data.on_demand_credits, next_credit_allocation_date: data.next_credit_allocation_date };
})
