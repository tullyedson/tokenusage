import { describe, expect, it } from "vitest";
import { escapeHtml, percent, resetLabel } from "./format";

describe("usage presentation", () => {
  it("escapes provider text instead of executing it", () => { expect(escapeHtml('<script>"x"</script>')).toBe("&lt;script&gt;&quot;x&quot;&lt;/script&gt;"); });
  it("keeps missing quotas different from depleted quotas", () => { expect(percent(null)).toBe("Unavailable"); expect(percent(0)).toBe("0% left"); expect(percent(75)).toBe("75% left"); });
  it("does not pretend an expired reading refilled", () => { expect(resetLabel(100, 200_000)).toBe("Reset time passed. Refresh to confirm."); });
});
