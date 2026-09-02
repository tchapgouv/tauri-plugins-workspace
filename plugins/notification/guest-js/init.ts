// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

import { invoke, addPluginListener } from '@tauri-apps/api/core'
import type { PermissionState } from '@tauri-apps/api/core'
import type { Options } from './index'

interface ActionPerformedPayload {
  actionId: string
  inputValue?: string
  notification?: {
    id: number
  }
}

;(function () {
  let permissionSettable = false
  let permissionValue = 'default'

  async function isPermissionGranted(): Promise<boolean> {
    // @ts-expect-error __TEMPLATE_windows__ will be replaced in rust before it's injected.
    if (window.Notification.permission !== 'default' || __TEMPLATE_windows__) {
      return await Promise.resolve(window.Notification.permission === 'granted')
    }
    return await invoke('plugin:notification|is_permission_granted')
  }

  function setNotificationPermission(value: NotificationPermission): void {
    permissionSettable = true
    // @ts-expect-error we can actually set this value on the webview
    window.Notification.permission = value
    permissionSettable = false
  }

  async function requestPermission(): Promise<PermissionState> {
    return await invoke<PermissionState>(
      'plugin:notification|request_permission'
    ).then((permission) => {
      setNotificationPermission(
        permission === 'prompt' || permission === 'prompt-with-rationale'
          ? 'default'
          : permission
      )
      return permission
    })
  }

  async function sendNotification(options: string | Options): Promise<void> {
    if (typeof options === 'object') {
      Object.freeze(options)
    }

    await invoke('plugin:notification|notify', {
      options:
        typeof options === 'string'
          ? {
              title: options
            }
          : options
    })
  }

  const registry = new Map<number, TauriNotification>()
  let nextId = 1
  let listenerPromise: Promise<unknown> | null = null

  function ensureListener() {
    listenerPromise ??= addPluginListener(
      'notification',
      'actionPerformed',
      (payload: ActionPerformedPayload) => {
        dispatch(payload)
      }
    ).catch((error) => {
      console.error('failed to listen to notification actions', error)
    })
    return listenerPromise
  }

  function dispatch({ actionId, notification }: ActionPerformedPayload) {
    const target = registry.get(notification?.id as number)
    if (!target) return
    if (actionId === 'dismiss') {
      registry.delete(notification!.id)
      target.dispatchEvent(new Event('close'))
    } else {
      target.dispatchEvent(new Event('click'))
    }
  }

  class TauriNotification extends EventTarget {
    readonly title: string
    readonly body: string
    readonly icon: string
    readonly silent: boolean
    readonly tag: string
    readonly data: unknown
    onclick: ((this: TauriNotification, ev: Event) => void) | null = null
    onclose: ((this: TauriNotification, ev: Event) => void) | null = null
    onerror: ((this: TauriNotification, ev: Event) => void) | null = null
    onshow: ((this: TauriNotification, ev: Event) => void) | null = null
    #id: number

    constructor(title: string, options: NotificationOptions = {}) {
      super()
      this.title = title
      this.body = options.body ?? ''
      this.icon = options.icon ?? ''
      this.silent = options.silent ?? false
      this.tag = options.tag ?? ''
      this.data = options.data ?? null
      this.#id = nextId++
      this.addEventListener('click', (ev) => this.onclick?.call(this, ev))
      this.addEventListener('close', (ev) => this.onclose?.call(this, ev))
      registry.set(this.#id, this)
      void ensureListener()
      void sendNotification({ ...options, id: this.#id, title } as Options)
    }

    close(): void {
      if (!registry.delete(this.#id)) return
      this.dispatchEvent(new Event('close'))
    }
  }

  // @ts-expect-error unfortunately we can't implement the whole type, so we overwrite it with our own version
  window.Notification = TauriNotification

  // @ts-expect-error tauri does not have sync IPC :(
  window.Notification.requestPermission = requestPermission

  Object.defineProperty(window.Notification, 'permission', {
    enumerable: true,
    get: () => permissionValue,
    set: (v) => {
      if (!permissionSettable) {
        throw new Error('Readonly property')
      }
      // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
      permissionValue = v
    }
  })

  void isPermissionGranted().then(function (response) {
    if (response === null) {
      setNotificationPermission('default')
    } else {
      setNotificationPermission(response ? 'granted' : 'denied')
    }
  })
})()
