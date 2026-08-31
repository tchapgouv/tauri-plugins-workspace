<script>
  import {
    sendNotification,
    registerActionTypes,
    setOnActionPerformed
  } from '@tauri-apps/plugin-notification'
  export let onMessage

  Notification.requestPermission();

  let sound = ''
  let actionTypesRegistered = false
  // send the notification directly
  // the backend is responsible for checking the permission
  function _sendNotification() {
    onMessage('send');
    sendNotification({
      title: 'Notification title',
      body: 'This is the notification body',
      sound: sound || null
    })
  }

  // alternatively, check the permission ourselves
  function triggerNotification() {
    if (Notification.permission === 'default') {
      onMessage('default');
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
      onMessage('granted');
      _sendNotification()
    } else {
      onMessage('Permission is denied')
    }
  }

  async function setupActionTypes() {
    await registerActionTypes([
      {
        id: 'message',
        actions: [{ id: 'reply', title: 'Reply' }]
      }
    ])
    actionTypesRegistered = true
    onMessage('Action types registered')
  }

  async function setupActionListener() {
    await setOnActionPerformed((action) => {
      onMessage(
        'Action performed: ' +
          action.actionId +
          ' on notification ' +
          action.notification.id
      )
    })
    onMessage('Action listener registered')
  }

  function sendActionNotification() {
    sendNotification({
      title: 'Action Notification',
      body: 'Click the Reply button or tap the body',
      actionTypeId: 'message'
    })
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

<button class="btn" on:click={setupActionTypes}>
  Register action types
</button>
<button class="btn" on:click={setupActionListener}>
  Set action listener
</button>
<button class="btn" on:click={sendActionNotification}>
  Send action notification
</button>
