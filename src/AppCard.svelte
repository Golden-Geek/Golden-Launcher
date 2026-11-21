<script lang="ts">
  import type { CardOutcome, Channel, InstalledVersion, InstallProgress, LauncherApp } from './api';

  interface Props {
    app: LauncherApp;
    progress?: InstallProgress;
    canceling: boolean;
    outcome?: CardOutcome;
    installDisabled: boolean;
    settingsDisabled: boolean;
    openingVersion?: string;
    onChannel: (channel: Channel) => void;
    onInstall: (graphical?: boolean) => void;
    onCancel: () => void;
    onOpen: (version: InstalledVersion) => void;
    onUninstall: (version: InstalledVersion) => void;
    onKeepCopy: (version: InstalledVersion) => void;
  }

  let {
    app, progress, canceling, outcome, installDisabled, settingsDisabled, openingVersion,
    onChannel, onInstall, onCancel, onOpen, onUninstall, onKeepCopy,
  }: Props = $props();

  const availabilityText = {
    available: '',
    previewOnly: 'Preview only',
    awaitingRelease: 'Awaiting release',
    unavailable: 'Unavailable on this device',
    unknown: 'Release status unknown',
  };
  const stages = {
    downloading: 'Downloading',
    verifying: 'Verifying package',
    installing: 'Installing',
    uninstalling: 'Uninstalling',
    retaining: 'Saving a copy',
  };
  const origins = { native: 'Installed app', appImage: 'AppImage', retainedCopy: 'Kept copy' };
  const actions = { install: 'Install', update: 'Update', alongside: 'Install alongside' };

  const percent = $derived(progress?.total ? Math.min(100, Math.floor(100 * progress.downloaded / progress.total)) : undefined);
  const installLabel = $derived(app.installAction && outcome?.kind !== 'restartRequired' ? actions[app.installAction] : undefined);
  const retry = $derived(outcome?.retryInstall);
  let confirmUninstall = $state<string>();

  function changeChannel(event: Event & { currentTarget: HTMLSelectElement }) {
    const selection = event.currentTarget;
    if (selection.value === 'stable' || selection.value === 'preview') {
      onChannel(selection.value);
    }
    selection.value = app.channel;
  }
</script>

<article class="app-card {app.id}" aria-labelledby="title-{app.id}">
  <div class="card-header">
    <div class="art"><img src="/apps/{app.id}.png" alt="" width="46" height="46" /></div>
    <div class="app-title">
      <h2 id="title-{app.id}">{app.name}</h2>
      <p>{app.description}</p>
    </div>
  </div>

  <div class="version-row">
    {#if app.installed}
      <span class="version installed" title="Verified installed version"><i></i> {app.installed.version} installed</span>
    {/if}
    {#if app.available && (!app.installed || app.available.version !== app.installed.version)}
      <span class:preview={app.available.preview} class="version">
        {app.available.version} {app.available.preview ? 'preview' : 'available'}
      </span>
    {:else if app.availability !== 'available'}
      <span class:preview={app.availability === 'previewOnly'} class="version">{availabilityText[app.availability]}</span>
    {/if}
    {#if app.stale}<span class="version cached">Cached</span>{/if}
  </div>

  {#if progress}
    <div class="operation">
      <div class="operation-label">
        <span role="status">{progress.stage === 'downloading' && canceling ? 'Cancelling…' : stages[progress.stage]}</span>
        {#if progress.stage === 'downloading' && percent !== undefined}<span>{percent}%</span>{/if}
      </div>
      <progress aria-label={stages[progress.stage]} max="100" value={progress.stage === 'downloading' ? percent : undefined}></progress>
    </div>
  {:else if outcome}
    <p class:failed={outcome.kind === 'failed'} class="outcome" role={outcome.kind === 'failed' ? 'alert' : 'status'}>{outcome.message}</p>
  {/if}

  <div class="actions">
    {#if progress?.stage === 'downloading'}
      <button class="button" disabled={canceling} onclick={onCancel}>Cancel</button>
    {:else if !progress}
      {#if app.installed && outcome?.kind !== 'restartRequired'}
        {@const installed = app.installed}
        <button class:primary={!installLabel} class="button" disabled={!!openingVersion} onclick={() => onOpen(installed)}>
          {openingVersion === installed.key ? 'Opening…' : 'Open'}<span aria-hidden="true">↗</span>
        </button>
      {/if}
      {#if installLabel}
        <button class="button primary" disabled={installDisabled} onclick={() => onInstall()}>
          {retry ? 'Retry' : installLabel}
        </button>
        {#if outcome?.retryGraphically}
          <button class="button" disabled={installDisabled} onclick={() => onInstall(true)}>Retry with installer</button>
        {/if}
      {:else if app.availability === 'previewOnly'}
        <button class="button" disabled={settingsDisabled} onclick={() => onChannel('preview')}>Enable Preview</button>
      {/if}
    {/if}
  </div>

  {#if app.installedVersions.length}
    <details>
      <summary>Manage versions{app.installedVersions.length > 1 ? ` (${app.installedVersions.length})` : ''}</summary>
      {#each app.installedVersions as version (version.key)}
        <div class="manage-version">
          <div class="installed-version-label">
            <strong>{version.version}</strong>
            <span>{origins[version.origin]}</span>
          </div>
          {#if confirmUninstall === version.key}
            <p>Uninstall {app.name} {version.version}?</p>
            <div class="version-controls">
              <button class="button danger" disabled={installDisabled} onclick={() => {
                confirmUninstall = undefined;
                onUninstall(version);
              }}>Uninstall version</button>
              <button class="button" onclick={() => { confirmUninstall = undefined; }}>Keep installed</button>
            </div>
          {:else}
            <div class="version-controls">
              <button class="button" disabled={!!progress || !!openingVersion} onclick={() => onOpen(version)}>
                {openingVersion === version.key ? 'Opening…' : 'Open'}
              </button>
              {#if version.canRetain}
                <button class="button" disabled={installDisabled} onclick={() => onKeepCopy(version)}>Keep a copy</button>
              {/if}
              {#if version.canUninstall}
                <button class="button" disabled={installDisabled} onclick={() => { confirmUninstall = version.key; }}>Uninstall…</button>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </details>
  {/if}

  <details>
    <summary>{app.available ? 'Release notes & settings' : 'Release settings'}</summary>
    <label class="channel">
      Release channel
      <select value={app.channel} disabled={settingsDisabled} onchange={changeChannel}>
        <option value="stable">Stable</option>
        <option value="preview">Preview</option>
      </select>
    </label>
    {#if app.available}
      <h3>Version {app.available.version}</h3>
      {#if app.available.notes.length}
        <ul class="notes">
          {#each app.available.notes as note}<li>{note}</li>{/each}
        </ul>
      {:else}
        <p>No release notes were provided.</p>
      {/if}
    {/if}
    {#if app.refreshError}
      <p class="refresh-error">{app.refreshError}</p>
    {/if}
  </details>
</article>
