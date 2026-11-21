import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type AppId =
  | 'chataigne' | 'chataigne2' | 'map-gyver' | 'map-gyver2'
  | 'blux' | 'blinder-kitten' | 'ben-to' | 'lgml';

export type Channel = 'stable' | 'preview';
export type Availability = 'available' | 'previewOnly' | 'awaitingRelease' | 'unavailable' | 'unknown';

export interface InstalledVersion {
  key: string;
  version: string;
  origin: 'native' | 'appImage' | 'retainedCopy';
  canUninstall: boolean;
  canRetain: boolean;
}

export interface LauncherApp {
  id: AppId;
  name: string;
  description: string;
  channel: Channel;
  availability: Availability;
  stale: boolean;
  refreshError: string | null;
  installed: InstalledVersion | null;
  installedVersions: InstalledVersion[];
  installAction: 'install' | 'update' | 'alongside' | null;
  available: { version: string; preview: boolean; notes: string[] } | null;
}

export interface InstallProgress {
  id: AppId;
  stage: 'downloading' | 'verifying' | 'installing' | 'uninstalling' | 'retaining';
  downloaded: number;
  total: number | null;
}

export interface InstallOutcome {
  kind: 'installed' | 'uninstalled' | 'retained' | 'cancelled' | 'restartRequired' | 'failed';
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
  open: (id: AppId, key: string) => invoke<void>('launch', { id, key }),
  uninstall: (id: AppId, key: string) => invoke<InstallOutcome>('uninstall', { id, key }),
  keepCopy: (id: AppId, key: string) => invoke<InstallOutcome>('retain_installation', { id, key }),
  onProgress: (handler: (progress: InstallProgress) => void) =>
    listen<InstallProgress>('install-progress', ({ payload }) => handler(payload)),
};
