# AGENTS.md

## Purpose
- This file is the working contract for coding agents in this repository.
- Prefer the checked-in build scripts, `DEVELOPING.md`, `.editorconfig`, and architecture tests over guesses.

## Repository map
- `komga/`: main Spring Boot backend server, APIs, static asset hosting, most Kotlin code.
- `komga-webui/`: Vue 2 + TypeScript frontend, built separately and copied into backend resources.
- `komga-tray/`: thin Kotlin/Compose desktop wrapper around the backend.
- `komga-rust/`: auxiliary code, not included by root `settings.gradle`.

## Rules files status
- No root `AGENTS.md` existed before this file.
- No `.cursor/rules/` directory was found.
- No `.cursorrules` file was found.
- No `.github/copilot-instructions.md` file was found.
- Do not assume hidden AI-specific rules beyond this file and checked-in project config.

## Toolchain and environment
- This repository uses `mise` for local tool version management.
- Root `mise.toml` pins Java to `zulu-25`.
- `komga-webui/mise.toml` pins Node to `18`.
- Prefer `mise install` / `mise use` compatible workflows instead of guessing local versions.
- Backend development requires Java JDK 21+ according to `DEVELOPING.md`.
- Frontend development requires Node 18+; check `.nvmrc`.
- Gradle wrapper version is 8.14.3.
- Kotlin plugin version at the root is 2.2.0.
- `komga` and `komga-tray` compile to JVM 17 bytecode even though local JDK should be 21+.

## Install and bootstrap
- If `mise` is available, install toolchains before running project commands.
- From repo root: `mise install`
- From `komga-webui/`: `mise install`
- Frontend dependencies: `cd komga-webui && npm install`
- Backend wrapper is checked in: `./gradlew`
- If frontend deps are missing, `npm run build`, `npm run lint`, and `npm run test:unit` will fail.
- If Java is missing, all Gradle tasks will fail immediately.

## High-value commands
- Full backend + project tests: `./gradlew test`
- Backend module only: `./gradlew :komga:test`
- Desktop wrapper tests/build hooks: `./gradlew :komga-tray:test`
- Kotlin lint: `./gradlew ktlintCheck`
- Auto-format Kotlin/KTS: `./gradlew ktlintFormat`
- Frontend lint: `cd komga-webui && npm run lint`
- Frontend unit tests: `cd komga-webui && npm run test:unit`
- Frontend production build: `cd komga-webui && npm run build`
- Local frontend dev server: `cd komga-webui && npm run serve`

## Run locally
- Backend dev run: `./gradlew bootRun --args='--spring.profiles.active=dev,noclaim'`
- Alternative profile activation on Linux: `SPRING_PROFILES_ACTIVE=dev,noclaim ./gradlew bootRun`
- Use `dev,localdb,noclaim` when you want a persistent local DB.
- The frontend dev server runs on `localhost:8081`.
- The backend dev profile enables CORS for the frontend dev server.

## Frontend/backend integration commands
- Build web UI and copy it into backend static resources: `./gradlew :komga:prepareThymeLeaf`
- This is required if you want Spring Boot to serve the latest frontend bundle.
- Docker packaging path depends on the copied frontend bundle.
- Docker packaging prep: `./gradlew jreleaserPackage`

## Single-test recipes
- Single backend test class:
  - `./gradlew :komga:test --tests 'org.gotson.komga.interfaces.api.rest.UserControllerTest'`
- Another backend example:
  - `./gradlew :komga:test --tests 'org.gotson.komga.architecture.CodingRulesTest'`
- Frontend single spec file:
  - `cd komga-webui && npm run test:unit -- tests/unit/functions/toc.spec.ts`
- Another frontend single spec example:
  - `cd komga-webui && npm run test:unit -- tests/unit/types/pageLoader.spec.ts`
- Prefer class/spec-file scope when iterating; it is more reliable than broad full-suite runs.

## Before opening a PR or declaring work done
- Run relevant narrow tests first.
- Then run the broader suite that covers the touched area.
- For Kotlin changes, at minimum run `./gradlew ktlintCheck` and the impacted Gradle tests.
- For frontend changes, at minimum run `npm run lint` and the impacted unit specs.
- If frontend assets affect what Spring serves, also run `./gradlew :komga:prepareThymeLeaf`.

## Commit and branch conventions
- Commit messages follow Conventional Commits.
- Keep scopes meaningful, because release tooling uses commit metadata.
- Do not invent ad hoc commit styles.

## Formatting rules
- Root `.editorconfig` is authoritative.
- Default text formatting: UTF-8, LF, trim trailing whitespace, final newline.
- Default indent is 4 spaces for generic files.
- Kotlin, KTS, JS, TS, JSON, YAML use 2-space indentation.
- General max line length is 120.
- Kotlin/KTS explicitly disable line-length enforcement in `.editorconfig`.
- Kotlin allows trailing commas, including call-site trailing commas.
- Ktlint rule `multiline-if-else` is disabled; do not “fix” code just to satisfy that rule.

## Import conventions
- Prefer explicit imports; existing Kotlin and TS code does not use wildcard imports.
- In frontend TS, prefer alias imports from `@/` for project-local modules.
- Keep imports grouped by source and keep them stable instead of churn-heavy reordering.
- Avoid unused imports; ESLint and ktlint should stay clean.

## Kotlin code style
- Prefer constructor injection in production code.
- Do not introduce Spring field injection in main code.
- Prefer `val` over `var` unless mutation is genuinely required.
- Prefer expression-bodied functions where they stay readable.
- Use trailing commas in multiline Kotlin literals and parameter lists to match existing style.
- Keep controller request/response DTO mapping near the interface layer.
- Use Kotlin null-safety deliberately; do not replace explicit nullable handling with unsafe `!!` unless unavoidable.

## Backend architecture rules
- Controllers must live under `..interfaces..` packages.
- Classes annotated with `@RestController` or `@Controller` must end with `Controller`.
- Domain model classes must not depend on `infrastructure`, `interfaces`, `domain.persistence`, or `domain.service`.
- Interface slices are expected not to depend on each other.
- In `..domain..service..` and `..application..service..`, do not name classes `*Service` or `*Manager`.
- Use intent-revealing names like `Lifecycle`, `Provider`, `Repository`, `Converter`, etc.

## Error-handling rules
- Do not throw generic exceptions from production code.
- Translate domain/application failures into API-facing HTTP errors at the controller boundary.
- Existing controllers commonly throw `ResponseStatusException` for HTTP concerns.
- Prefer domain-specific exceptions such as duplicate/not-found variants over vague failures.
- Preserve useful context in error messages.
- Do not print to stdout/stderr from application code.
- Do not introduce `java.util.logging` or Joda-Time.

## Testing conventions
- Backend tests use JUnit 5.
- Backend assertions favor AssertJ.
- Do not add `org.junit.jupiter.api.Assertions` usage in tests; architecture tests forbid it.
- Spring integration tests commonly use `@SpringBootTest`, `@AutoConfigureMockMvc`, and `@ActiveProfiles("test")`.
- Frontend tests use Jest via Vue CLI.
- Keep new tests close to the existing module conventions instead of inventing a new test style.

## TypeScript and Vue style
- `komga-webui/tsconfig.json` has `strict: true`; keep new TS code strict.
- Prefer explicit return types on service methods and non-trivial exported functions.
- Prefer imported DTO/types over `any`.
- `useUnknownInCatchVariables` is disabled, but do not abuse loose catch typing.
- Frontend ESLint requires single quotes.
- Frontend ESLint forbids semicolons.
- Frontend ESLint requires trailing commas for multiline structures.
- `no-console` and `no-debugger` are errors in production mode.

## Frontend naming and structure
- Vue SFC views/components use PascalCase filenames, e.g. `HomeView.vue`.
- Service and plugin files commonly use kebab-case plus suffixes, e.g. `komga-users.service.ts`.
- TS type files under `src/types/` also commonly use kebab-case names.
- Reuse the established suffixes: `.service.ts`, `.plugin.ts`, `*View.vue`, `*Dialog.vue`, `*Controller.kt`, `*Dto.kt`.
- Preserve the existing package and folder boundaries unless the change truly requires a move.

## Practical coding guidance
- Match the local style of the file you edit before applying global preferences.
- Keep patches narrow; avoid opportunistic renames or formatting-only churn.
- When editing backend APIs, check matching DTOs, services, tests, and frontend consumers.
- When editing frontend services, check matching types, plugins, and the consuming views/components.
- When changing architecture-sensitive code, run the architecture tests, not just feature tests.

## What to read first for unfamiliar work
- `DEVELOPING.md` for setup and workflow.
- `build.gradle.kts` and `komga/build.gradle.kts` for task wiring.
- `.editorconfig` and `komga-webui/.eslintrc.js` for formatting/lint behavior.
- `komga/src/test/kotlin/org/gotson/komga/architecture/*.kt` for enforced architectural rules.

## Default agent behavior in this repo
- Do not assume missing commands; derive them from Gradle, npm scripts, and docs.
- Do not bypass lint or tests without saying so explicitly.
- Do not add new framework conventions when an existing repo convention already covers the case.
- If a command fails because dependencies are missing, report that separately from code correctness.
