import { describe, expect, it } from "vitest";
import { base64ToBytes, bytesToBase64 } from "./base64";

describe("base64 helpers", () => {
  it("round-trips arbitrary bytes", () => {
    const original = new Uint8Array([0, 1, 2, 250, 251, 252, 253, 254, 255, 42]);
    const encoded = bytesToBase64(original);
    const decoded = base64ToBytes(encoded);
    expect(Array.from(decoded)).toEqual(Array.from(original));
  });

  it("round-trips UTF-8 encoded text, including multi-byte characters", () => {
    const text = "hello ssh 🔒";
    const bytes = new TextEncoder().encode(text);
    const decoded = base64ToBytes(bytesToBase64(bytes));
    expect(new TextDecoder().decode(decoded)).toBe(text);
  });

  it("handles an empty buffer", () => {
    expect(bytesToBase64(new Uint8Array())).toBe("");
    expect(base64ToBytes("").length).toBe(0);
  });
});
