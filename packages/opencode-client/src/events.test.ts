import { describe, expect, it } from "vitest";
import { sidecarEventName, STREAM_RESUMED } from "./events.js";

describe("sidecarEventName", () => {
  it("matches the Rust rule: dots become underscores so Tauri accepts the name", () => {
    expect(sidecarEventName("message.part.updated")).toBe("sidecar:message_part_updated");
    expect(sidecarEventName("permission.v2.asked")).toBe("sidecar:permission_v2_asked");
    expect(sidecarEventName("session.idle")).toBe("sidecar:session_idle");
  });

  it("names the synthetic gap marker the same way Rust does", () => {
    expect(sidecarEventName("stream.resumed")).toBe(STREAM_RESUMED);
  });
});
