import { describe, expect, it } from "vitest";
import { hostsIp, hostsLine, isLocalhostName, reachableAsLocalhost } from "./localHostname";

describe("local hostname helpers", () => {
  it("maps the bind address, wildcard to loopback", () => {
    expect(hostsIp("127.0.0.5")).toBe("127.0.0.5");
    expect(hostsIp("0.0.0.0")).toBe("127.0.0.1");
    expect(hostsIp("::")).toBe("::1");
    expect(hostsLine("127.0.0.5", " IFG-Portainer.local ")).toBe("127.0.0.5\tifg-portainer.local");
  });

  it("recognises *.localhost names and the binds they reach", () => {
    expect(isLocalhostName("ifg-portainer.localhost")).toBe(true);
    expect(isLocalhostName("ifg-portainer.local")).toBe(false);
    expect(reachableAsLocalhost("127.0.0.1")).toBe(true);
    expect(reachableAsLocalhost("127.0.0.5")).toBe(false);
  });
});
