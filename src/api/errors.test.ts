import { describe, expect, it } from "vitest";
import { errorMessage, parseHostKeyError } from "./errors";

describe("parseHostKeyError", () => {
  it("extracts an untrusted host-key decision", () => {
    const decision = parseHostKeyError({
      kind: "host_key_untrusted",
      message: {
        host: "example.com",
        port: 22,
        key_algorithm: "ssh-ed25519",
        fingerprint_sha256: "SHA256:abc",
        host_key_b64: "base64",
      },
    });
    expect(decision).toEqual({
      kind: "untrusted",
      host: "example.com",
      port: 22,
      keyAlgorithm: "ssh-ed25519",
      fingerprint: "SHA256:abc",
      hostKeyB64: "base64",
    });
  });

  it("extracts a changed host-key decision with both fingerprints", () => {
    const decision = parseHostKeyError({
      kind: "host_key_mismatch",
      message: {
        host: "example.com",
        port: 22,
        key_algorithm: "ssh-ed25519",
        previous_fingerprint_sha256: "SHA256:old",
        presented_fingerprint_sha256: "SHA256:new",
        host_key_b64: "base64",
      },
    });
    expect(decision?.kind).toBe("changed");
    expect(decision?.previousFingerprint).toBe("SHA256:old");
    expect(decision?.fingerprint).toBe("SHA256:new");
  });

  it("returns null for unrelated errors", () => {
    expect(parseHostKeyError({ kind: "authentication_failed" })).toBeNull();
    expect(parseHostKeyError(new Error("boom"))).toBeNull();
    expect(parseHostKeyError("plain string")).toBeNull();
  });
});

describe("errorMessage", () => {
  it("uses a friendly label for known error kinds", () => {
    expect(errorMessage({ kind: "incorrect_password" })).toMatch(/incorrect/i);
  });

  it("formats a rate-limited error with the retry delay", () => {
    expect(errorMessage({ kind: "rate_limited", message: { retry_after_secs: 8 } })).toContain("8s");
  });

  it("passes through a plain string message", () => {
    expect(errorMessage({ kind: "validation", message: "port must be between 1 and 65535" })).toBe(
      "port must be between 1 and 65535",
    );
  });

  it("falls back to the error's own message for non-AppError values", () => {
    expect(errorMessage(new Error("network down"))).toBe("network down");
  });
});
