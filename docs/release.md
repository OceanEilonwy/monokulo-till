# Releasing Till for iPhone and iPad

[`release-ios.yml`](../.github/workflows/release-ios.yml) builds the core,
archives the app with its extensions, and uploads the build to TestFlight
with `xcodebuild`. Signing is automatic and managed in Apple's cloud,
through an App Store Connect API key, so no certificate or `.p12` is
stored anywhere. The build number is the workflow's run number. The
version is `MARKETING_VERSION` in [`ios/project.yml`](../ios/project.yml).
This page lists the one-time setup, in order (docs/ios-plan.md §5, phase I7).

## 1. Apple Developer (developer.apple.com › Account)

1. **Team ID**: Membership details › Team ID (10 characters). It is
   `APPLE_TEAM_ID` below.
2. **App Group**: Certificates, Identifiers & Profiles › Identifiers › `+` ›
   App Groups › `group.com.monokulo.till` (description "Monokulo Till").
3. **App IDs**: Identifiers › `+` › App IDs › App, with an explicit
   bundle ID. Xcode's automatic signing registers these itself and turns on
   what the entitlements ask for. Register them by hand anyway: App Store
   Connect needs the app's ID before the app record can exist (§2), and
   doing all three now means the first run can't fail on a capability.
   | Bundle ID | Description | Capabilities |
   | --- | --- | --- |
   | `com.monokulo.till` | Monokulo Till | App Groups (`group.com.monokulo.till`), Associated Domains, Push Notifications, Time Sensitive Notifications |
   | `com.monokulo.till.notifications` | Monokulo Till Notifications | App Groups (`group.com.monokulo.till`), Time Sensitive Notifications |
   | `com.monokulo.till.widgets` | Monokulo Till Widgets | none (the widget extension has no entitlements) |

   To assign an App Group, tick App Groups, choose **Configure**, then pick
   `group.com.monokulo.till`. The shared Keychain group needs no
   capability: it is prefixed by the team ID and works under any App ID of
   the team.
4. **Devices**: Devices › `+`, then add the iPhone's and the iPad's UDIDs
   (Finder › the device › click the serial number). Xcode signs the archive
   for development before re-signing it for the App Store, and a
   development profile needs at least one registered device.

## 2. App Store Connect (appstoreconnect.apple.com)

1. **The app**: Apps › `+` › New App. Platform iOS, name
   **Monokulo Till**, primary language English (U.S.), bundle ID
   `com.monokulo.till`, SKU `monokulo-till`, User Access Full Access.
2. **The API key**: Users and Access › Integrations › App Store Connect
   API › **Team Keys** › `+`. Name it "Till CI" and give it the role
   **Admin**. Making cloud-managed distribution certificates and
   registering App IDs and profiles needs Admin. An Individual key can't
   provision. Download `AuthKey_<KEY ID>.p8`; Apple allows only one
   download. Note the **Key ID** and the **Issuer ID** shown above the list.

## 3. GitHub (OceanEilonwy/monokulo-till › Settings)

1. **Variable**: Secrets and variables › Actions › Variables › New
   repository variable `APPLE_TEAM_ID` = the team ID. It must be a
   repository variable, not an environment one, because Pages reads it too
   (§5).
2. **Environment**: Environments › New environment `testflight`. Add
   yourself as a required reviewer. Under deployment branches and tags,
   choose selected ones: `main` and the tag rule `v*`.
3. **Secrets** in the `testflight` environment:
   | Secret | Value |
   | --- | --- |
   | `ASC_KEY_ID` | the Key ID, e.g. `2X9R4HXF34` |
   | `ASC_ISSUER_ID` | the Issuer ID, a UUID |
   | `ASC_KEY_P8` | the `.p8` file's whole text, as it is (not base64), from `-----BEGIN PRIVATE KEY-----` to `-----END PRIVATE KEY-----` |

   Or with `gh`:
   ```sh
   gh variable set APPLE_TEAM_ID --body ABCDE12345
   gh secret set ASC_KEY_ID --env testflight --body 2X9R4HXF34
   gh secret set ASC_ISSUER_ID --env testflight --body 00000000-0000-0000-0000-000000000000
   gh secret set ASC_KEY_P8 --env testflight < AuthKey_2X9R4HXF34.p8
   ```
4. **Variables** in the `testflight` environment: the points of sale's
   public application ids, which the build puts in the app's Info.plist
   (`ios/Till/Build.swift`). Both sign in with PKCE, so there's no secret.
   | Variable | Value |
   | --- | --- |
   | `TILL_SQUARE_APPLICATION_ID` | the Square application's id, `sq0idp-…` (or `sandbox-sq0idb-…`) |
   | `TILL_SQUARE_SANDBOX` | `true` for a sandbox id, else `false` |
   | `TILL_CLOVER_APPLICATION_ID` | the Clover app's App ID |
   | `TILL_CLOVER_ENVIRONMENT` | `north_america`, `europe`, `latin_america` or `sandbox` |
   | `TILL_RELAY_URL` | the push relay's https address, such as `https://push.monokulo.com` |
   | `TILL_BELL_URL` | the Bell's https address, such as `https://bell.monokulo.com` |
   | `TILL_LINK_HOST` | the host of the app's universal links and sign-in redirects, such as `till.monokulo.com` (§5) |

   ```sh
   gh variable set TILL_SQUARE_APPLICATION_ID --env testflight --body sq0idp-…
   gh variable set TILL_SQUARE_SANDBOX --env testflight --body false
   gh variable set TILL_CLOVER_APPLICATION_ID --env testflight --body …
   gh variable set TILL_CLOVER_ENVIRONMENT --env testflight --body north_america
   gh variable set TILL_RELAY_URL --env testflight --body https://push.monokulo.com
   gh variable set TILL_BELL_URL --env testflight --body https://bell.monokulo.com
   gh variable set TILL_LINK_HOST --env testflight --body till.monokulo.com
   ```
   The iOS app signs in through universal links on `till.monokulo.com`.
   A production Square application's OAuth redirect URL must be
   `https://till.monokulo.com/square`, and a production Clover app's Site
   URL `https://till.monokulo.com/clover`. With a sandbox id
   (`TILL_SQUARE_SANDBOX=true`, `TILL_CLOVER_ENVIRONMENT=sandbox`) the app
   signs in through the debug pages instead, `/square/debug/` and
   `/clover/debug/`, as Android's debug builds do: Square takes one redirect
   URL per environment, so the sandbox application keeps `/square/debug/`
   for both platforms. The page forwards an iPhone or iPad to
   `monokulo-till://` and Android to its debug build. These are not the
   Bell's webhook URLs (`/v1/square/production/webhook` on the Bell's own
   host), which are set on Square's webhook subscription.

   The workflow's first step stops with an error naming any of these that is
   missing or malformed, before any macOS minutes are spent on a build.

## 4. A build on TestFlight

GitHub offers **Run workflow** only for workflows on the default branch,
so `release-ios.yml` must be on `main` first (merge the pull request that
brings it).

1. Actions › **iOS release** › Run workflow, on `main`. Approve the
   `testflight` deployment. Pushing a tag `v<MARKETING_VERSION>` (e.g.
   `v0.1.0`) does the same, and the run fails if the tag and the version
   differ.
2. The run archives the app, exports a signed `.ipa` once to check every
   bundle's entitlements (`aps-environment` must be `production`) and
   version, then uploads. App Store Connect takes about 5 to 30 minutes to
   process the build. If the run fails, its `till-ios-release-logs-<n>`
   artifact has the xcodebuild and distribution logs. dSYMs are kept as
   `till-ios-dsyms-<n>`.
3. **Export compliance**: on the first build, TestFlight asks about
   encryption (the build shows "Missing Compliance"). Answer it once in
   App Store Connect › the build. To stop the question, set
   `INFOPLIST_KEY_ITSAppUsesNonExemptEncryption` in project.yml to match
   your answer.
4. **Testers**: TestFlight › Internal Testing › `+` group "Owner". Add
   yourself and turn on automatic distribution. Install TestFlight on the
   iPhone and the iPad, signed in with that Apple Account, and install
   Till from it.
5. For each build after that, start a **new** run. Re-running a run that
   already uploaded sends the same build number again, which App Store
   Connect refuses. For a new version, change `MARKETING_VERSION` and tag
   `v<version>`.

Each run's archive makes a new Apple Development certificate on the
runner. Revoke old "Created via API" development certificates in
Certificates now and then. The distribution certificate is cloud-managed
and shared by all runs.

## 5. till.monokulo.com and universal links

The README notes that the site is published by GitHub Pages at
`till.monokulo.com` ([README](../README.md#building-the-android-app)).
To set it up:

1. DNS for monokulo.com: a `CNAME` record `till` → `oceaneilonwy.github.io`.
2. Settings › Pages: Source **GitHub Actions**, Custom domain
   `till.monokulo.com`, then **Enforce HTTPS** once the certificate is
   issued.
3. After `APPLE_TEAM_ID` is set, run Actions › **Pages** › Run workflow.
   It writes the team into `site/.well-known/apple-app-site-association`
   (`TEAMID.com.monokulo.till` → `<team>.com.monokulo.till`) as it
   deploys. Without the variable it deploys with the placeholder and warns.
4. Check `https://till.monokulo.com/.well-known/apple-app-site-association`,
   then Apple's cached copy,
   `https://app-site-association.cdn-apple.com/a/v1/till.monokulo.com`,
   which can lag by hours. A device fetches it when Till is installed.

## 6. What the test needs from the relay and the Bell

| Works now, without the relay or the Bell | Needs a deployed relay and Bell with APNs keys (phase I5) |
| --- | --- |
| iPad in Slide Over: the shift runs in the foreground on its 2 s timer, finds sales, and shows codes. A sale found while tucked away raises a time-sensitive local alert | Dings: alert pushes that wake a till that isn't running, and the Notification Service Extension's "Monero sale · $24.50" rewrite |
| iPhone with Till open: the shift's timer finds sales, and the Live Activity starts and updates from the app while it runs | Live Activity updates while Till is in the background or closed (ActivityKit push tokens through the relay) |
| Onboarding, connect links (once §5 is done), Square and Clover sign-in, the test sale | The 30 s timer with dings in place of the 2 s one |

A TestFlight build is a production build, so its pushes use APNs
production. The relay's APNs key must therefore send to production, not
the sandbox.
