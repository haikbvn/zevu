const vscode = require("vscode");
const cp = require("child_process");
const path = require("path");

const DEFAULT_CLI_PATH = "/Applications/Zevu.app/Contents/MacOS/zevu";
let extensionDisposable;

function runFocusHook(pid) {
  const cliPath = vscode.workspace
    .getConfiguration("zevu")
    .get("cliPath", DEFAULT_CLI_PATH);
  if (!path.isAbsolute(cliPath)) {
    logError("zevu.cliPath must be an absolute path to the Zevu CLI");
    return;
  }
  cp.execFile(
    cliPath,
    ["hook", "keyboard-focus-changed", vscode.env.uriScheme, String(pid)],
    (err) => {
      if (err) {
        logError(err);
      }
    },
  );
}

function log(...args) {
  // zevu.log is an internal developer setting, not shown in the settings editor.
  if (vscode.workspace.getConfiguration("zevu").get("log") === true) {
    console.log(`zevu: ${args[0]}`, args.slice(1));
  }
}

function logError(message) {
  console.error(`zevu: ${message}`);
}

function updateActiveTerminal(terminal) {
  const activeTerminal = terminal || vscode.window.activeTerminal;

  if (!activeTerminal) {
    noActiveTerminals();
    return;
  }
  activeTerminal.processId.then((processId) => {
    if (processId) {
      runFocusHook(processId);
    }
  });
}

function noActiveTerminals() {
  log("no active terminals");
}

function activate(context) {
  const environment = context.environmentVariableCollection;
  environment.persistent = false;
  environment.replace("Q_NEW_SESSION", "1");

  extensionDisposable = vscode.Disposable.from(
    new vscode.Disposable(() => environment.clear()),
    vscode.window.onDidOpenTerminal(() => {
      log("Terminal opened. Total count: " + vscode.window.terminals.length);
    }),
    vscode.window.onDidCloseTerminal(() => {
      log("Terminal closed. Total count: " + vscode.window.terminals.length);

      if (vscode.window.terminals.length === 0) {
        noActiveTerminals();
      } else {
        updateActiveTerminal();
      }
    }),
    vscode.window.onDidChangeActiveTerminal((terminal) => {
      log("Active terminal changed");
      updateActiveTerminal(terminal);
    }),
    vscode.window.onDidChangeActiveTextEditor(() => {
      runFocusHook(0);
    }),
    vscode.window.onDidChangeTextEditorSelection(() => {
      runFocusHook(0);
    }),
  );
  context.subscriptions.push(extensionDisposable);
  updateActiveTerminal();
}

function deactivate() {
  extensionDisposable?.dispose();
  extensionDisposable = undefined;
}

module.exports = {
  activate,
  deactivate,
};
