import logger from "loglevel";
import {
  SETTINGS,
  updateSettings,
} from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { SpecLocationSource } from "@aws/amazon-q-developer-cli-shared/utils";
import {
  getSpecPath,
  importSpecFromLocation,
  loadFigSubcommand,
  loadSubcommandCached,
} from "../src/loadSpec";
import * as loadHelpers from "../src/loadHelpers";
import { specCache } from "../src/caches";
import {
  expect,
  it,
  beforeAll,
  describe,
  beforeEach,
  vi,
  Mock,
  afterEach,
} from "vitest";

const { importSpecFromFile } = loadHelpers;

vi.mock("../src/loadHelpers", () => ({
  importSpecFromFile: vi
    .fn()
    .mockResolvedValue({ default: { name: "loadFromFile" } }),
  importFromBundledSpecs: vi
    .fn()
    .mockResolvedValue({ default: { name: "loadFromBundle" } }),
  bundledSpecExists: vi.fn().mockResolvedValue(true),
  importFromLocalhost: vi
    .fn()
    .mockResolvedValue({ default: { name: "loadFromDevServer" } }),
  isDiffVersionedSpec: vi.fn(),
  getVersionFromFullFile: vi.fn().mockResolvedValue("2.0.0"),
}));

vi.mock("@aws/amazon-q-developer-cli-api-bindings-wrappers", async () => ({
  ...(await vi.importActual(
    "@aws/amazon-q-developer-cli-api-bindings-wrappers",
  )),
  executeCommand: vi.fn(),
}));

// TODO: remove this statement and move fig dir to shared
const FIG_DIR = "~/.fig";

beforeAll(() => {
  updateSettings({});
});

describe("getSpecPath", () => {
  const cwd = "test_cwd";

  it("works", async () => {
    expect(await getSpecPath("git", cwd)).toEqual({
      type: SpecLocationSource.GLOBAL,
      name: "git",
    });
  });

  it("works for specs containing a slash in the name", async () => {
    expect(
      await getSpecPath("@withfig/autocomplete-tools", cwd, false),
    ).toEqual({
      type: SpecLocationSource.GLOBAL,
      name: "@withfig/autocomplete-tools",
    });
  });

  it("works for scripts containing a slash in the name", async () => {
    expect(await getSpecPath("@withfig/autocomplete-tools", cwd)).toEqual({
      type: SpecLocationSource.LOCAL,
      name: "autocomplete-tools",
      path: `${cwd}/@withfig/`,
    });
  });

  it("works properly with local commands", async () => {
    expect(await getSpecPath("./test", cwd)).toEqual({
      type: SpecLocationSource.LOCAL,
      name: "test",
      path: `${cwd}/`,
    });
    expect(await getSpecPath("~/test", cwd)).toEqual({
      type: SpecLocationSource.LOCAL,
      path: `~/`,
      name: "test",
    });
    expect(await getSpecPath("/test", cwd)).toEqual({
      type: SpecLocationSource.LOCAL,
      path: `/`,
      name: "test",
    });
    expect(await getSpecPath("/dir/test", cwd)).toEqual({
      type: SpecLocationSource.LOCAL,
      path: `/dir/`,
      name: "test",
    });
    expect(await getSpecPath("~/dir/test", cwd)).toEqual({
      type: SpecLocationSource.LOCAL,
      path: `~/dir/`,
      name: "test",
    });
    expect(await getSpecPath("./dir/test", cwd)).toEqual({
      type: SpecLocationSource.LOCAL,
      path: `${cwd}/dir/`,
      name: "test",
    });
  });

  it("works properly with ? commands", async () => {
    expect(await getSpecPath("?", cwd)).toEqual({
      type: SpecLocationSource.LOCAL,
      path: `${cwd}/`,
      name: "_shortcuts",
    });
  });

  it("works properly with + commands", async () => {
    expect(await getSpecPath("+", cwd)).toEqual({
      type: SpecLocationSource.LOCAL,
      name: "+",
      path: "~/",
    });
  });
});

describe("loadFigSubcommand", () => {
  window.URL.createObjectURL = vi.fn();

  beforeEach(() => {
    (loadHelpers.isDiffVersionedSpec as Mock).mockResolvedValue(false);
    updateSettings({});
  });

  afterEach(() => {
    (loadHelpers.isDiffVersionedSpec as Mock).mockClear();
  });

  it("works with expected input", async () => {
    const result = await loadFigSubcommand({
      name: "path",
      type: SpecLocationSource.LOCAL,
    });
    expect(loadHelpers.isDiffVersionedSpec).toHaveBeenCalledTimes(1);
    expect(result.name).toBe("loadFromFile");
  });

  it("works in dev mode", async () => {
    const devPath = "~/some-folder/";
    const specLocation: Fig.SpecLocation = {
      name: "git",
      type: SpecLocationSource.LOCAL,
    };

    updateSettings({
      [SETTINGS.DEV_COMPLETIONS_FOLDER]: devPath,
      [SETTINGS.DEV_MODE_NPM]: false,
      [SETTINGS.DEV_MODE]: false,
    });
    await loadFigSubcommand(specLocation);
    expect(importSpecFromFile).toHaveBeenLastCalledWith(
      "git",
      `${FIG_DIR}/autocomplete/build/`,
      logger,
    );

    updateSettings({
      [SETTINGS.DEV_COMPLETIONS_FOLDER]: devPath,
      [SETTINGS.DEV_MODE_NPM]: true,
      [SETTINGS.DEV_MODE]: false,
    });
    await loadFigSubcommand(specLocation);
    expect(importSpecFromFile).toHaveBeenLastCalledWith("git", devPath, logger);

    updateSettings({
      [SETTINGS.DEV_COMPLETIONS_FOLDER]: devPath,
      [SETTINGS.DEV_MODE_NPM]: false,
      [SETTINGS.DEV_MODE]: true,
    });
    await loadFigSubcommand(specLocation);
    expect(importSpecFromFile).toHaveBeenLastCalledWith("git", devPath, logger);

    updateSettings({
      [SETTINGS.DEV_COMPLETIONS_FOLDER]: "~/some-folder/",
      [SETTINGS.DEV_MODE_NPM]: false,
      [SETTINGS.DEV_MODE]: true,
    });
    await loadFigSubcommand(specLocation);
    expect(importSpecFromFile).toHaveBeenLastCalledWith("git", devPath, logger);

    expect(loadHelpers.isDiffVersionedSpec).toHaveBeenCalledTimes(4);
  });
});

describe("bundled catalogue precedence", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    updateSettings({});
    vi.mocked(loadHelpers.bundledSpecExists).mockResolvedValue(true);
    vi.mocked(importSpecFromFile).mockResolvedValue({
      default: { name: "loadFromFile" },
    });
  });

  it("loads global specs and nested version files from the bundle", async () => {
    const result = await importSpecFromLocation({
      name: "aws",
      type: SpecLocationSource.GLOBAL,
      diffVersionedFile: "2.0.0",
    });
    expect(loadHelpers.importFromBundledSpecs).toHaveBeenCalledWith(
      "aws/2.0.0",
    );
    expect(result.specFile.default).toEqual({ name: "loadFromBundle" });
    expect(importSpecFromFile).not.toHaveBeenCalled();
  });

  it("keeps local project specs ahead of the global bundle", async () => {
    await importSpecFromLocation({
      name: "_shortcuts",
      path: "/project/",
      type: SpecLocationSource.LOCAL,
    });
    expect(importSpecFromFile).toHaveBeenCalledWith(
      "_shortcuts",
      "/project/.fig/autocomplete/build/",
      logger,
    );
    expect(loadHelpers.bundledSpecExists).not.toHaveBeenCalled();
    expect(loadHelpers.importFromBundledSpecs).not.toHaveBeenCalled();
  });

  it("preserves the personal fallback for specs absent from the bundle", async () => {
    vi.mocked(loadHelpers.bundledSpecExists).mockResolvedValue(false);
    await importSpecFromLocation({
      name: "personal",
      type: SpecLocationSource.GLOBAL,
    });
    expect(importSpecFromFile).toHaveBeenCalledWith(
      "personal",
      "~/.fig/autocomplete/build/",
      logger,
    );
    expect(loadHelpers.importFromBundledSpecs).not.toHaveBeenCalled();
  });

  it("keeps a development folder override ahead of the bundle", async () => {
    updateSettings({
      [SETTINGS.DEV_MODE]: true,
      [SETTINGS.DEV_COMPLETIONS_FOLDER]: "/dev/specs/",
    });
    await importSpecFromLocation({
      name: "git",
      type: SpecLocationSource.GLOBAL,
    });
    expect(importSpecFromFile).toHaveBeenCalledWith(
      "git",
      "/dev/specs/",
      logger,
    );
    expect(loadHelpers.bundledSpecExists).not.toHaveBeenCalled();
  });

  it("keeps the explicit development server ahead of folder overrides", async () => {
    updateSettings({
      [SETTINGS.DEV_MODE]: true,
      [SETTINGS.DEV_COMPLETIONS_FOLDER]: "/dev/specs/",
      [SETTINGS.DEV_COMPLETIONS_SERVER_PORT]: 3000,
    });
    await importSpecFromLocation({
      name: "git",
      type: SpecLocationSource.GLOBAL,
    });
    expect(loadHelpers.importFromLocalhost).toHaveBeenCalledWith("git", 3000);
    expect(importSpecFromFile).not.toHaveBeenCalled();
    expect(loadHelpers.bundledSpecExists).not.toHaveBeenCalled();
  });

  it("falls back to the bundle when a development folder spec is missing", async () => {
    updateSettings({
      [SETTINGS.DEV_MODE]: true,
      [SETTINGS.DEV_COMPLETIONS_FOLDER]: "/dev/specs/",
    });
    vi.mocked(importSpecFromFile).mockRejectedValueOnce(new Error("missing"));
    const result = await importSpecFromLocation({
      name: "git",
      type: SpecLocationSource.GLOBAL,
    });
    expect(result.specFile.default).toEqual({ name: "loadFromBundle" });
    expect(loadHelpers.importFromBundledSpecs).toHaveBeenCalledWith("git");
  });

  it("propagates a bundled module error instead of falling back to a personal spec", async () => {
    const failure = new Error("invalid bundled module");
    vi.mocked(loadHelpers.importFromBundledSpecs).mockRejectedValueOnce(failure);
    const localLogger = { ...logger, error: vi.fn() };
    await expect(importSpecFromLocation({
      name: "git",
      type: SpecLocationSource.GLOBAL,
    }, localLogger)).rejects.toBe(failure);
    expect(localLogger.error).toHaveBeenCalledWith(
      "Unable to load bundled spec",
      failure,
    );
    expect(importSpecFromFile).not.toHaveBeenCalled();
  });

  it("retains the missing-spec error after the personal fallback fails", async () => {
    vi.mocked(loadHelpers.bundledSpecExists).mockResolvedValue(false);
    vi.mocked(importSpecFromFile).mockRejectedValueOnce(new Error("missing"));
    await expect(importSpecFromLocation({
      name: "missing",
      type: SpecLocationSource.GLOBAL,
    })).rejects.toThrow("No spec found");
  });

  it("resolves a versioned module with the CLI version and preserves its generator", async () => {
    const generator: Fig.Generator = { script: ["git", "branch"] };
    const subcommand: Fig.Subcommand = {
      name: "git",
      args: { generators: generator },
      subcommands: [{ name: "checkout" }],
    };
    const getSpec = vi.fn().mockReturnValue(subcommand);
    vi.mocked(loadHelpers.isDiffVersionedSpec).mockResolvedValueOnce(false);
    vi.mocked(loadHelpers.importFromBundledSpecs).mockResolvedValueOnce({
      default: getSpec,
    });
    const result = await loadFigSubcommand({
      name: "git",
      type: SpecLocationSource.GLOBAL,
    });
    expect(getSpec).toHaveBeenCalledWith("2.0.0");
    expect(result).toBe(subcommand);
    expect(result.args).toBe(subcommand.args);
  });

  it("rejects an invalid diff-versioned file after resolving its bundled path", async () => {
    vi.mocked(loadHelpers.isDiffVersionedSpec).mockResolvedValueOnce(true);
    vi.mocked(loadHelpers.importFromBundledSpecs)
      .mockResolvedValueOnce({
        default: () => ({
          versionedSpecPath: "aws/2.0.0",
          version: "2.1.0",
        }),
      })
      .mockResolvedValueOnce({ default: { name: "aws" } });
    await expect(loadFigSubcommand({
      name: "aws",
      type: SpecLocationSource.GLOBAL,
    })).rejects.toThrow("Invalid versioned specs file");
    expect(loadHelpers.importFromBundledSpecs).toHaveBeenNthCalledWith(
      1,
      "aws/index",
    );
    expect(loadHelpers.importFromBundledSpecs).toHaveBeenNthCalledWith(
      2,
      "aws/2.0.0",
    );
  });
});

describe("loadSubcommandCached", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    specCache.clear();
    updateSettings({});
    vi.mocked(loadHelpers.isDiffVersionedSpec).mockResolvedValue(false);
    vi.mocked(loadHelpers.bundledSpecExists).mockResolvedValue(true);
  });

  it("reuses the parser cache and isolates command names", async () => {
    const location = { name: "git", type: SpecLocationSource.GLOBAL };
    const first = await loadSubcommandCached(location);
    const second = await loadSubcommandCached(location);
    expect(second).toBe(first);
    expect(loadHelpers.importFromBundledSpecs).toHaveBeenCalledTimes(1);

    await loadSubcommandCached({
      name: "npm",
      type: SpecLocationSource.GLOBAL,
    });
    expect(loadHelpers.importFromBundledSpecs).toHaveBeenCalledTimes(2);
  });

  it("continues to resolve diff-versioned catalogue indexes", async () => {
    vi.mocked(loadHelpers.isDiffVersionedSpec).mockResolvedValue(true);
    await loadSubcommandCached({
      name: "aws",
      type: SpecLocationSource.GLOBAL,
    });
    expect(loadHelpers.importFromBundledSpecs).toHaveBeenCalledWith("aws/index");
  });

  it("does not load a bundled module for a disabled command", async () => {
    updateSettings({ [SETTINGS.DISABLE_FOR_COMMANDS]: ["git"] });
    await expect(loadSubcommandCached({
      name: "git",
      type: SpecLocationSource.GLOBAL,
    })).rejects.toThrow("Command requested disabled completion spec");
    expect(loadHelpers.bundledSpecExists).not.toHaveBeenCalled();
    expect(loadHelpers.importFromBundledSpecs).not.toHaveBeenCalled();
  });
});
