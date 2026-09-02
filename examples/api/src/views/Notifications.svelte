<script>
  export let onMessage

  let sound = ''

  function triggerNotification() {
    if (Notification.permission === 'default') {
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

  function _sendNotification() {
    const notification = new Notification('Notification title', {
      body: 'This is the notification body',
      sound: sound || undefined
    })

    notification.onclick = function () {
      onMessage('notification onclick')
    }

    notification.onclose = function () {
      onMessage('notification onclose')
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
