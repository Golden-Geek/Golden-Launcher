<script lang="ts">
  import type { CardOutcome, Channel, InstallProgress, LauncherApp } from './api';

  interface Props {
    app: LauncherApp;
    progress?: InstallProgress;
    canceling: boolean;
    outcome?: CardOutcome;
    installDisabled: boolean;
    settingsDisabled: boolean;
    isOpening: boolean;
    onChannel: (channel: Channel) => void;
    onInstall: (graphical?: boolean) => void;
    onCancel: () => void;
    onOpen: () => void;
    onUninstall: () => void;
  }

  let {
    app, progress, canceling, outcome, installDisabled, settingsDisabled, isOpening,
    onChannel, onInstall, onCancel, onOpen, onUninstall,
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
  };

  const percent = $derived(progress?.total ? Math.min(100, Math.floor(100 * progress.downloaded / progress.total)) : undefined);
  const canInstall = $derived(app.available && (!app.installed || app.updateAvailable) && outcome?.kind !== 'restartRequired');
  const retry = $derived(outcome?.retryInstall);
  let confirmUninstall = $state(false);

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
    {#if app.available && (!app.installed || app.updateAvailable || app.available.version !== app.installed.version)}
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
        <button class:primary={!canInstall} class="button" disabled={isOpening} onclick={onOpen}>
          {isOpening ? 'Opening…' : 'Open'}<span aria-hidden="true">↗</span>
        </button>
      {/if}
      {#if canInstall}
        <button class="button primary" disabled={installDisabled} onclick={() => onInstall()}>
          {retry ? 'Retry' : app.updateAvailable ? 'Update' : 'Install'}
        </button>
        {#if outcome?.retryGraphically}
          <button class="button" disabled={installDisabled} onclick={() => onInstall(true)}>Retry with installer</button>
        {/if}
      {:else if app.availability === 'previewOnly'}
        <button class="button" disabled={settingsDisabled} onclick={() => onChannel('preview')}>Enable Preview</button>
      {/if}
    {/if}
  </div>

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
    {#if app.installed?.canUninstall}
      <div class="manage-version">
        {#if confirmUninstall}
          <p>Uninstall {app.name} {app.installed.version}?</p>
          <button class="button danger" disabled={installDisabled} onclick={() => {
            confirmUninstall = false;
            onUninstall();
          }}>Uninstall version</button>
          <button class="button" onclick={() => { confirmUninstall = false; }}>Keep installed</button>
        {:else}
          <button class="button" disabled={installDisabled} onclick={() => { confirmUninstall = true; }}>Uninstall…</button>
        {/if}
      </div>
    {/if}
  </details>
</article>
