<script>
  import { onDestroy } from 'svelte'
  import {
    sendNotification,
    registerActionTypes,
    onAction,
  } from '@tauri-apps/plugin-notification'
  export let onMessage

  let sound = ''
  let unlistenAction
  
  Notification.requestPermission()

  async function init() {
                  onMessage('init')

    // Register the action type first (required before sendNotification with actionTypeId,
    // and awaited so the desktop backend knows about it before any notification is sent).
    await registerActionTypes([
      {
        id: 'clawterm-default',
        actions: [{ id: 'open', title: 'Open', foreground: true }],
      },
    ])

    // Keep the returned PluginListener so we can unregister it on unmount
    // (avoids listener leaks on remount / HMR).
    unlistenAction = await onAction((notification) => {
      onMessage('onAction is ' + JSON.stringify(notification))
    })
  }

  init()

  onDestroy(() => {
    unlistenAction?.unregister()
  })

  // send the notification directly
  // the backend is responsible for checking the permission
  function _sendNotification() {
    sendNotification({
      title: 'Notification title',
      body: 'This is the notification body',
      sound: sound || null,
      actionTypeId: 'clawterm-default',
    })
  }

  // alternatively, check the permission ourselves
  function triggerNotification() {
    if (Notification.permission === 'default') {
        onMessage('default')
      Notification.requestPermission()
        .then(function (response) {
          if (response === 'granted') {
            _sendNotification()
          } else {
            onMessage('Permission is ' + response)
          }
        })
        .catch(onMessage)
    } else if (Notification.permission === 'granted') {
              onMessage('granted')

      _sendNotification()
    } else {
      onMessage('Permission is denied')
    }
  }
</script>

<input
  class="input grow"
  placeholder="Notification sound..."
  bind:value={sound}
/>
<button class="btn" id="notification" on:click={triggerNotification}>
  Send test notification
</button>
