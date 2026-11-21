import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type AppId =
  | 'chataigne' | 'chataigne2' | 'map-gyver' | 'map-gyver2'
  | 'blux' | 'blinder-kitten' | 'ben-to' | 'lgml';

export type Channel = 'stable' | 'preview';
export type Availability = 'available' | 'previewOnly' | 'awaitingRelease' | 'unavailable' | 'unknown';

export interface LauncherApp {
  id: AppId;
  name: string;
  description: string;
  channel: Channel;
  availability: Availability;
  stale: boolean;
  refreshError: string | null;
  installed: { version: string; canUninstall: boolean } | null;
  updateAvailable: boolean;
  available: { version: string; preview: boolean; notes: string[] } | null;
}

export interface InstallProgress {
  id: AppId;
  stage: 'downloading' | 'verifying' | 'installing' | 'uninstalling';
  downloaded: number;
  total: number | null;
}

export interface InstallOutcome {
  kind: 'installed' | 'uninstalled' | 'cancelled' | 'restartRequired' | 'failed';
  message: string;
  retryGraphically: boolean;
}

export type CardOutcome = InstallOutcome & { retryInstall?: boolean };

export const launcher = {
  library: () => invoke<LauncherApp[]>('library'),
  refresh: () => invoke<LauncherApp[]>('refresh'),
  setChannel: (id: AppId, channel: Channel) => invoke<LauncherApp[]>('set_channel', { id, channel }),
  install: (id: AppId, version: string, graphical: boolean) =>
    invoke<InstallOutcome>('install', { id, version, graphical }),
  cancel: (id: AppId) => invoke<boolean>('cancel_download', { id }),
  open: (id: AppId) => invoke<void>('launch', { id }),
  uninstall: (id: AppId, version: string) => invoke<InstallOutcome>('uninstall', { id, version }),
  onProgress: (handler: (progress: InstallProgress) => void) =>
    listen<InstallProgress>('install-progress', ({ payload }) => handler(payload)),
};
