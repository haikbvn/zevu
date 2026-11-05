# Autocomplete parser

Global completion specs and their index are loaded from the bundled catalogue
through `spec://localhost/`. The desktop protocol serves files from the app's
`Resources/specs` directory, including nested and diff-versioned modules. Runtime
catalogue loading never falls back to an HTTP CDN.

Spec resolution keeps upstream precedence: an explicitly configured development
server, then the development completions folder, then the requested project-local
spec or global bundled spec. Commands absent from the bundled index retain the
personal `~/.fig/autocomplete/build/` fallback. The development server is used
only when development mode and its port setting are enabled.

The parser retains its index, CLI-version, and converted-spec caches, version
resolution, disabled-command settings, and generator definitions. A failed index
load is not cached as a successful catalogue; a later lookup can recover using
the same bundled protocol.

Bundling makes static completion independent of AWS services and catalogue
downloads. Generators still use the upstream process bridge and real shell
context. Individual catalogue generators may query the network themselves;
their output is not synthesized when a command or service is unavailable.
