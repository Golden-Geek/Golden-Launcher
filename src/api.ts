import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type AppId =
  | 'chataigne' | 'chataigne2' | 'map-gyver' | 'map-gyver2'
  | 'blux' | 'blinder-kitten' | 'ben-to' | 'lgml' | 'snoring-pony';

export type Channel = 'stable' | 'beta';
export type Availability = 'available' | 'awaitingRelease' | 'unavailable' | 'unknown';

export type ReleaseAction =
  | { kind: 'install' | 'update' | 'alongside' | 'installed' | 'unavailable' }
  | { kind: 'replace'; key: string; version: string };

export interface Release {
  version: string;
  notes: string[];
  action: ReleaseAction;
}

export interface VersionCursor {
  channel: Channel;
  version: string;
}

export interface PublishedVersion {
  channel: Channel;
  release: Release;
}

interface VersionPage {
  versions: PublishedVersion[];
  next: VersionCursor | null;
  incomplete: boolean;
}

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
  availability: Availability;
  stale: boolean;
  refreshError: string | null;
  installed: InstalledVersion | null;
  installedVersions: InstalledVersion[];
  releases: Record<Channel, Release | null>;
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

export type CardOutcome = InstallOutcome & { retryFor?: { channel: Channel; version: string } };

export const selfUpdater = {
  prepare: () => invoke<string | null>('prepare_launcher_update'),
  install: () => invoke<void>('install_launcher_update'),
};

export const launcher = {
  library: () => invoke<LauncherApp[]>('library'),
  refresh: () => invoke<LauncherApp[]>('refresh'),
  versions: (id: AppId, after: VersionCursor | null) => invoke<VersionPage>('release_versions', { id, after }),
  install: (id: AppId, channel: Channel, release: Release, graphical: boolean) =>
    invoke<InstallOutcome>('install', {
      id, channel, version: release.version, graphical,
      replaceKey: release.action.kind === 'replace' ? release.action.key : null,
    }),
  cancel: (id: AppId) => invoke<boolean>('cancel_download', { id }),
  open: (id: AppId, key: string) => invoke<void>('launch', { id, key }),
  uninstall: (id: AppId, key: string) => invoke<InstallOutcome>('uninstall', { id, key }),
  keepCopy: (id: AppId, key: string) => invoke<InstallOutcome>('retain_installation', { id, key }),
  onProgress: (handler: (progress: InstallProgress) => void) =>
    listen<InstallProgress>('install-progress', ({ payload }) => handler(payload)),
};
