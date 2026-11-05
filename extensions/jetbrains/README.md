# Zevu Terminal Companion for JetBrains

This project uses [Gradle](https://gradle.org/) as a package manager and a builder/runner.

The plugin ID is `dev.zevu.jetbrains`, and the companion version is `0.2.0`.
Terminal focus notifications invoke `/Applications/Zevu.app/Contents/MacOS/zevu`
directly, so the IDE does not need the CLI on its `PATH`. The upstream Java package
and terminal notification protocol are unchanged.

## How it works
The extension registers a [service](src/main/java/io/fig/jetbrains/FigService.java) which keeps track of the current terminals.

A [ToolWindowManager listener](src/main/java/io/fig/jetbrains/FigWindowListener.java) is also registered, which allows us to get the ContentManager of the terminal ToolWindow.

This ContentManager is used through two [ITerminalEntry](src/main/java/io/fig/jetbrains/entries/ITerminalEntry.java):
- [ExistingTerminalsEntry](src/main/java/io/fig/jetbrains/entries/impl/ExistingTerminalsEntry.java) fetch the currently opened terminals and send me to the service
- [ListenTerminalsEntry](src/main/java/io/fig/jetbrains/entries/impl/ListenTerminalsEntry.java) listen for creating or deleting terminals and also send them to the service

The service finally handles those two entries and processes the logic to run the right commands at the right moment.

## Setup
1. Clone this repo and import it to your IDE as a **Gradle** project.
2. Use Java 17 to run the plugin in a test IDE with `./gradlew runIde`.
> Note: you may need to run `./gradlew clean` before this, or the plugin will be marked as incompatible with your IDE.
3. On macOS, build the plugin with `JAVA_HOME="$(brew --prefix openjdk@17)/libexec/openjdk.jdk/Contents/Home" ./gradlew buildPlugin`.
4. You can see all tasks with `./gradlew tasks` (or `.\gradlew.bat tasks` on Windows)

You can also run the tasks directly inside IntelliJ IDEA, in the Gradle panel:

![](gradle-panel.png)

## Packaging for the macOS app

`buildPlugin` produces `build/distributions/zevu-terminal-companion-0.2.0.zip`.
Extract the instrumented companion JAR from the ZIP's `lib/` directory and copy it
to `../../crates/fig_integrations/src/intellij/instrumented-zevu-terminal-companion-0.2.0.jar`
before compiling the Rust installer. Use the instrumented JAR from the plugin ZIP,
not the uninstrumented Java build output. The app build entrypoint performs this
packaging step; no pre-existing Amazon plugin binary is reused.

The installer preserves all upstream JetBrains variants and their application
folder discovery. It installs and removes only the Zevu companion, leaving Fig,
Amazon Q, CodeWhisperer, and other plugins untouched.
