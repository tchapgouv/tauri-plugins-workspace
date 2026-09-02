![plugin-notification](https://github.com/tauri-apps/plugins-workspace/raw/v2/plugins/notification/banner.png)

Send message notifications (brief auto-expiring OS window element) to your user. Can also be used with the Notification Web API.

| Platform | Supported |
| -------- | --------- |
| Linux    | ✓         |
| Windows  | ✓         |
| macOS    | ✓         |
| Android  | ✓         |
| iOS      | ✓         |

## Install

_This plugin requires a Rust version of at least **1.77.2**_

There are three general methods of installation that we can recommend.

1. Use crates.io and npm (easiest, and requires you to trust that our publishing pipeline worked)
2. Pull sources directly from Github using git tags / revision hashes (most secure)
3. Git submodule install this repo in your tauri project and then use file protocol to ingest the source (most secure, but inconvenient to use)

Install the Core plugin by adding the following to your `Cargo.toml` file:

`src-tauri/Cargo.toml`

```toml
[dependencies]
tauri-plugin-notification = "2.0.0"
# alternatively with Git:
tauri-plugin-notification = { git = "https://github.com/tauri-apps/plugins-workspace", branch = "v2" }
```

You can install the JavaScript Guest bindings using your preferred JavaScript package manager:

```sh
pnpm add @tauri-apps/plugin-notification
# or
npm add @tauri-apps/plugin-notification
# or
yarn add @tauri-apps/plugin-notification
```

## Usage

First you need to register the core plugin with Tauri:

`src-tauri/src/lib.rs`

```rust
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Then you need to add the permissions to your capabilities file:

`src-tauri/capabilities/main.json`

```json
{
  ...
  "permissions": [
    ...
    "notification:default"
  ],
  ...
}
```

Afterwards all the plugin's APIs are available through the JavaScript guest bindings:

```javascript
import {
  isPermissionGranted,
  requestPermission,
  sendNotification
} from '@tauri-apps/plugin-notification'

async function checkPermission() {
  if (!(await isPermissionGranted())) {
    return (await requestPermission()) === 'granted'
  }
  return true
}

export async function enqueueNotification(title, body) {
  if (!(await checkPermission())) {
    return
  }
  sendNotification({ title, body })
}
```

### Notification with Sound

You can add sound to your notifications on all platforms (desktop and mobile):

```javascript
import { sendNotification } from '@tauri-apps/plugin-notification'
import { platform } from '@tauri-apps/api/os'

// Basic notification with sound
sendNotification({
  title: 'New Message',
  body: 'You have a new message',
  sound: 'notification.wav' // Path to sound file
})

// Platform-specific sounds
async function sendPlatformSpecificNotification() {
  const platformName = platform()

  let soundPath
  if (platformName === 'darwin') {
    // On macOS: use system sounds or sound files in the app bundle
    soundPath = 'Ping' // macOS system sound
  } else if (platformName === 'linux') {
    // On Linux: use XDG theme sounds or file paths
    soundPath = 'message-new-instant' // XDG theme sound
  } else {
    // On Windows: use file paths
    soundPath = 'notification.wav'
  }

  sendNotification({
    title: 'Platform-specific Notification',
    body: 'This notification uses platform-specific sound',
    sound: soundPath
  })
}
```

### Web Notification API

On desktop the plugin injects a `window.Notification` shim that implements the standard Web Notification API, so code like this works unchanged:

```javascript
const notification = new Notification('Hello', { body: 'World' })
notification.onclick = () => console.log('clicked')
notification.onclose = () => console.log('closed')
notification.close()
```

**Caveats:**

- `close()` removes the JS-side handlers but does **not** withdraw an already displayed OS notification (not supported on Windows; retaining the handle on macOS would prevent the notification from appearing).
- `onshow` and `onerror` never fire.
- `tag` does not coalesce notifications.
- `onclick` also fires for custom action buttons, since the Web API has a single activation concept.
- On **Windows**, click/dismiss callbacks only work for **installed** apps with an AppUserModelID shortcut. In development (`cargo run`) the toast shows the PowerShell identity and callbacks may never fire.
- On **macOS** in dev mode, notifications are attributed to `com.apple.Terminal`.
- Each visible notification holds one blocked OS thread until it is acted on or expires.

## Contributing

PRs accepted. Please make sure to read the Contributing Guide before making a pull request.

## Partners

<table>
  <tbody>
    <tr>
      <td align="center" valign="middle">
        <a href="https://crabnebula.dev" target="_blank">
          <img src="https://github.com/tauri-apps/plugins-workspace/raw/v2/.github/sponsors/crabnebula.svg" alt="CrabNebula" width="283">
        </a>
      </td>
    </tr>
  </tbody>
</table>

For the complete list of sponsors please visit our [website](https://tauri.app#sponsors) and [Open Collective](https://opencollective.com/tauri).

## License

Code: (c) 2015 - Present - The Tauri Programme within The Commons Conservancy.

MIT or MIT/Apache 2.0 where applicable.
