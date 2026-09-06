(async function () {
  const response = await fetch("/settings", { credentials: "include", cache: "no-store", signal: AbortSignal.timeout(15000) });
  if (!response.ok || !response.url.startsWith("https://ollama.com/settings")) throw new Error("Sign in to Ollama, then refresh.");
  const html = await response.text();
  const doc = new DOMParser().parseFromString(html, "text/html");
  const labels = ["Monthly usage", "Session usage", "Hourly usage", "Weekly usage"];
  const windows = [];
  for (const label of labels) {
    const leaf = Array.from(doc.querySelectorAll("span, p, h2, h3, h4, div")).find(element => element.children.length === 0 && element.textContent.trim() === label);
    if (!leaf) continue;
    let block = leaf.parentElement;
    for (let i = 0; block && i < 5; i++, block = block.parentElement) {
      const text = block.textContent.replace(/\s+/g, " ");
      if (labels.some(other => other !== label && text.includes(other))) break;
      const dollars = text.match(/\$([\d,]+(?:\.\d+)?)\s+of\s+\$([\d,]+(?:\.\d+)?)\s+used/i);
      const percent = text.match(/([\d.]+)\s*%\s*used/i);
      const bar = block.querySelector('[role="progressbar"], [style*="width:"]');
      const width = bar && (bar.getAttribute("aria-valuenow") || (bar.getAttribute("style") || "").match(/width:\s*([\d.]+)%/)?.[1]);
      if (!dollars && !percent && !width) continue;
      const time = block.querySelector("[data-time], time[datetime]");
      const reset = time && (time.getAttribute("data-time") || time.getAttribute("datetime"));
      if (dollars) windows.push({ label, used: Number(dollars[1].replaceAll(",", "")), limit: Number(dollars[2].replaceAll(",", "")), resetsAt: reset });
      else windows.push({ label, usedPercent: Number(percent ? percent[1] : width), resetsAt: reset });
      break;
    }
  }
  if (!windows.length) throw new Error("Ollama's usage section was not found. Sign in and open Settings. If it is visible there, the provider reader may need updating.");
  return { windows };
})
