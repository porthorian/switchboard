# Switchboard macOS Release Runbook

## Scope

This runbook covers Milestone 10 release readiness tasks for macOS:

- build/release artifact generation
- code signing
- notarization
- stapling
- smoke validation before publish

## Prerequisites

- Apple Developer account with notarization access
- Xcode command line tools installed
- App-specific signing identities available in keychain
- CEF distribution available and tested with the target build
- Clean release branch with all tests passing

## 1. Build And Test Gate

Run from repository root:

```bash
cargo test -p switchboard-core
cargo test -p switchboard-app
cargo build --release -p switchboard-app
```

Gate:

- no failing tests
- no release build errors

## 2. Create `.app` Bundle Layout

Use your packaging script/process to produce:

- `Switchboard.app/Contents/MacOS/switchboard-app`
- `Switchboard.app/Contents/Frameworks/Chromium Embedded Framework.framework`
- required CEF helper resources/subprocess artifacts
- `Info.plist` with correct bundle id/version/build

Validate bundle structure:

```bash
plutil -lint Switchboard.app/Contents/Info.plist
```

## 3. Sign Bundle

Sign nested components first, then the app bundle.

Example:

```bash
codesign --force --timestamp --options runtime --sign "Developer ID Application: <TEAM>" \
  "Switchboard.app/Contents/Frameworks/Chromium Embedded Framework.framework"

codesign --force --timestamp --options runtime --entitlements entitlements.plist \
  --sign "Developer ID Application: <TEAM>" "Switchboard.app"
```

Verify:

```bash
codesign --verify --deep --strict --verbose=2 "Switchboard.app"
spctl --assess --type execute --verbose=4 "Switchboard.app"
```

## 4. Notarize

Submit:

```bash
xcrun notarytool submit "Switchboard.zip" \
  --apple-id "<APPLE_ID>" \
  --team-id "<TEAM_ID>" \
  --password "<APP_SPECIFIC_PASSWORD>" \
  --wait
```

Check history/logs if needed:

```bash
xcrun notarytool history --apple-id "<APPLE_ID>" --team-id "<TEAM_ID>" --password "<APP_SPECIFIC_PASSWORD>"
```

## 5. Staple

```bash
xcrun stapler staple "Switchboard.app"
xcrun stapler validate "Switchboard.app"
```

## 6. Smoke Checks (Required)

Run these checks on a clean machine/user profile:

- app launches without Gatekeeper quarantine/signature errors
- window renders UI shell and active content tab on startup
- profile/workspace/tab CRUD works
- close/reopen restores persisted state
- resize persistence works across restarts
- no crash when switching profiles/workspaces repeatedly
- no unexpected prompt bridge errors in console
- no crash on right click/context interactions

## 7. Publish Gate

Release is blocked if any of the following fails:

- signing verification
- notarization success
- stapling validation
- smoke check list

## 8. Rollback Procedure

- keep previous signed/notarized artifact available
- if smoke or production regression appears, revert release tag and publish previous artifact
- open incident issue with:
  - failing version/build
  - repro steps
  - logs/crash report
  - mitigation status
