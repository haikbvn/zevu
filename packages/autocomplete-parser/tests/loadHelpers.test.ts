import * as wrappers from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import * as loadHelpers from "../src/loadHelpers";
import { vi, describe, beforeEach, afterEach, it, expect } from "vitest";

const { getVersionFromFullFile } = loadHelpers;

const specData = {
  getVersionCommand: vi.fn().mockReturnValue(Promise.resolve("1.0.0")),
  default: () => {},
};

describe("test `getVersionFromFullFile`", () => {
  beforeEach(() => {
    vi.spyOn(loadHelpers, "getCachedCLIVersion").mockReturnValue(null);
  });
  afterEach(() => {
    vi.clearAllMocks();
  });
  it("missing `getVersionCommand` and working `command --version`", async () => {
    vi.spyOn(wrappers, "executeCommand").mockReturnValue(
      Promise.resolve({
        status: 0,
        stdout: "2.0.0",
        stderr: "",
      }),
    );
    const newSpecData = { ...specData, getVersionCommand: undefined };
    const version = await getVersionFromFullFile(newSpecData, "fig");
    expect(version).toEqual("2.0.0");
  });

  it("missing `getVersionCommand` and not working `command --version`", async () => {
    vi.spyOn(wrappers, "executeCommand").mockReturnValue(
      Promise.resolve({
        status: 1,
        stdout: "",
        stderr: "No command available.",
      }),
    );
    const newSpecData = { ...specData, getVersionCommand: undefined };
    const version = await getVersionFromFullFile(newSpecData, "npm");
    expect(version).toBeUndefined();
  });

  it("missing `getVersionCommand` and throwing `command --version`", async () => {
    vi.spyOn(wrappers, "executeCommand").mockReturnValue(Promise.reject());
    const newSpecData = { ...specData, getVersionCommand: undefined };
    const version = await getVersionFromFullFile(newSpecData, "npm");
    expect(version).toBeUndefined();
  });

  it("working `getVersionCommand`", async () => {
    vi.spyOn(wrappers, "executeCommand").mockReturnValue(
      Promise.resolve({
        status: 0,
        stdout: "",
        stderr: "No command available.",
      }),
    );
    const version = await getVersionFromFullFile(specData, "node");
    expect(version).toEqual("1.0.0");
  });
});

describe("bundled spec index", () => {
  const index = {
    completions: ["git", "npm", "aws"],
    diffVersionedCompletions: ["aws"],
  };
  const fetchIndex = vi.fn();

  beforeEach(() => {
    loadHelpers.clearSpecIndex();
    fetchIndex.mockReset();
    vi.stubGlobal("fetch", fetchIndex);
    fetchIndex.mockResolvedValue({
      ok: true,
      json: async () => index,
    });
  });

  afterEach(() => {
    loadHelpers.clearSpecIndex();
    vi.unstubAllGlobals();
  });

  it("uses the bundled protocol and shares the parsed index cache", async () => {
    expect(await loadHelpers.bundledSpecExists("git")).toBe(true);
    expect(await loadHelpers.bundledSpecExists("missing")).toBe(false);
    expect(await loadHelpers.isDiffVersionedSpec("aws")).toBe(true);
    expect(await loadHelpers.isDiffVersionedSpec("git")).toBe(false);
    expect(fetchIndex).toHaveBeenCalledExactlyOnceWith(
      "spec://localhost/index.json",
    );
  });

  it("shares one in-flight index request across simultaneous lookups", async () => {
    const results = await Promise.all([
      loadHelpers.bundledSpecExists("git"),
      loadHelpers.bundledSpecExists("npm"),
      loadHelpers.isDiffVersionedSpec("aws"),
    ]);
    expect(results).toEqual([true, true, true]);
    expect(fetchIndex).toHaveBeenCalledExactlyOnceWith(
      "spec://localhost/index.json",
    );
  });

  it("clears the existing parser index cache explicitly", async () => {
    await loadHelpers.bundledSpecExists("git");
    loadHelpers.clearSpecIndex();
    await loadHelpers.bundledSpecExists("npm");
    expect(fetchIndex).toHaveBeenCalledTimes(2);
  });

  it("does not treat an unsuccessful response as a valid catalogue", async () => {
    const json = vi.fn().mockResolvedValue(index);
    fetchIndex.mockResolvedValueOnce({ ok: false, status: 404, json });
    expect(await loadHelpers.bundledSpecExists("git")).toBe(false);
    expect(json).not.toHaveBeenCalled();
    // Preserve the upstream cache's recovery after a failed index load.
    expect(await loadHelpers.bundledSpecExists("git")).toBe(true);
    expect(fetchIndex).toHaveBeenCalledTimes(2);
    expect(fetchIndex.mock.calls.every(
      ([url]) => url === "spec://localhost/index.json",
    )).toBe(true);
  });

  it("validates the catalogue schema before caching it", async () => {
    fetchIndex.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ completions: ["git"] }),
    });
    expect(await loadHelpers.bundledSpecExists("git")).toBe(false);
    expect(await loadHelpers.isDiffVersionedSpec("aws")).toBe(true);
    expect(fetchIndex).toHaveBeenCalledTimes(2);
  });

  it.each(["transport", "json"])(
    "recovers from a failed %s load without an HTTP fallback",
    async (failure) => {
      if (failure === "transport") {
        fetchIndex.mockRejectedValueOnce(new Error("resource unavailable"));
      } else {
        fetchIndex.mockResolvedValueOnce({
          ok: true,
          json: async () => {
            throw new SyntaxError("invalid bundled index");
          },
        });
      }
      expect(await loadHelpers.bundledSpecExists("git")).toBe(false);
      expect(fetchIndex).toHaveBeenCalledTimes(1);
      expect(await loadHelpers.bundledSpecExists("git")).toBe(true);
      expect(fetchIndex).toHaveBeenCalledTimes(2);
      expect(fetchIndex.mock.calls.every(
        ([url]) => url === "spec://localhost/index.json",
      )).toBe(true);
    },
  );
});
