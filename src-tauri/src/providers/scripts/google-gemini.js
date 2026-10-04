(async function () {
  // Google's usage page calls BardFrontendService.GetUsageInfo (jSf9Qc).
  // Keep its session/CSRF data inside this isolated Google webview.
  if (location.hostname !== "gemini.google.com" || location.protocol !== "https:") {
    throw new Error("Sign in to Gemini with your Google AI account, then refresh.");
  }
  const token = window.WIZ_global_data?.SNlM0e;
  if (typeof token !== "string" || !token) throw new Error("Sign in to Gemini, then refresh.");
  const body = new URLSearchParams({
    "f.req": JSON.stringify([[["jSf9Qc", "[]", null, "generic"]]]),
    at: token
  });
  const response = await fetch("/_/BardChatUi/data/batchexecute?rpcids=jSf9Qc&source-path=%2Fusage&hl=en", {
    method: "POST", credentials: "include", cache: "no-store", redirect: "error",
    headers: { "Content-Type": "application/x-www-form-urlencoded;charset=UTF-8" },
    body: body.toString(), signal: AbortSignal.timeout(15000)
  });
  if (!response.ok) throw new Error("Gemini could not return usage. Open Connect account to check your sign-in.");
  const raw = await response.text();
  if (raw.length > 1024 * 1024) throw new Error("Gemini returned an oversized usage response.");
  let payload;
  for (const line of raw.split("\n")) {
    if (!line.trim().startsWith("[")) continue;
    let rows;
    try { rows = JSON.parse(line); } catch { continue; }
    if (!Array.isArray(rows)) continue;
    for (const row of rows) {
      if (Array.isArray(row) && row[0] === "wrb.fr" && row[1] === "jSf9Qc" && typeof row[2] === "string") {
        try { payload = JSON.parse(row[2]); } catch { /* Static error below. */ }
      }
    }
  }
  if (!Array.isArray(payload) || !Array.isArray(payload[1])) {
    throw new Error("Gemini's usage format was not recognized. Check your sign-in or update AI Usage.");
  }
  const windows = [];
  let credits = null;
  for (const bucket of payload[1]) {
    if (!Array.isArray(bucket)) continue;
    if (bucket[2] === 1 || bucket[2] === 2) {
      const used = bucket[1];
      if (typeof used !== "number" || !Number.isFinite(used) || used < 0 || used > 1) {
        throw new Error("Gemini returned an invalid usage fraction. Refresh or update AI Usage.");
      }
      // UsageWindow's fourth field holds a reset message whose first field is
      // google.protobuf.Timestamp, represented as [seconds, nanoseconds].
      const seconds = bucket[3]?.[0]?.[0];
      const reset = (typeof seconds === "string" && /^\d+$/.test(seconds)) ? Number(seconds) : seconds;
      windows.push({ kind: bucket[2] === 1 ? "five_hour" : "weekly", usedFraction: used,
        resetsAt: Number.isSafeInteger(reset) && reset > 0 ? reset : null });
    } else if (bucket[2] === 3) {
      const amount = bucket[0];
      const n = typeof amount === "number" ? amount : typeof amount === "string" && /^\d+(\.\d+)?$/.test(amount) ? Number(amount) : NaN;
      if (Number.isFinite(n) && n >= 0) credits = n;
    }
  }
  if (!windows.length) throw new Error("No Gemini allowances were returned. Open Connect account and check Usage.");
  const plans = { 2: "Google AI Pro", 3: "Google AI Ultra", 6: "Google AI Ultra", 4: "Google AI Plus" };
  return { source: "gemini", plan: plans[payload[0]] ?? null, windows, credits };
})
