<script lang="ts">
  import { tick } from 'svelte';
  import { launcher, type CardOutcome, type Channel, type InstalledVersion, type InstallProgress, type LauncherApp, type PublishedVersion, type Release, type VersionCursor } from './api';

  interface Props {
    app: LauncherApp;
    progress?: InstallProgress;
    canceling: boolean;
    outcome?: CardOutcome;
    installDisabled: boolean;
    openingVersion?: string;
    onInstall: (channel: Channel, release: Release, graphical?: boolean) => void;
    onCancel: () => void;
    onOpen: (version: InstalledVersion) => void;
    onUninstall: (version: InstalledVersion) => void;
    onKeepCopy: (version: InstalledVersion) => void;
  }

  let {
    app, progress, canceling, outcome, installDisabled, openingVersion,
    onInstall, onCancel, onOpen, onUninstall, onKeepCopy,
  }: Props = $props();

  const availabilityText = {
    available: 'No build available',
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
  const channels = ['stable', 'beta'] as const;
  const channelName = { stable: 'Stable', beta: 'Beta' };
  const actions = {
    install: 'Install', update: 'Update', alongside: 'Install alongside', replace: 'Replace…',
    installed: 'Installed', unavailable: 'Switch unavailable',
  };

  const percent = $derived(progress?.total ? Math.min(100, Math.floor(100 * progress.downloaded / progress.total)) : undefined);
  const missingBuild = $derived(app.stale ? 'No cached build' : availabilityText[app.availability]);
  const canOpen = $derived(!!app.installed && !progress && outcome?.kind !== 'restartRequired');
  let confirmUninstall = $state<string>();
  let panel = $state<'versions' | 'catalog' | 'updates' | 'replace'>();
  let replacement = $state<{ channel: Channel; version: string; key: string; graphical: boolean; fromCatalog: boolean }>();
  let archived = $state<PublishedVersion[]>([]);
  let nextVersion = $state<VersionCursor | null>(null);
  let loadingVersions = $state(false);
  let incompleteVersions = $state(false);
  let versionError = $state('');
  let catalogRequest = 0;
  let dialog: HTMLDialogElement;

  const latest = $derived(channels.flatMap((channel) => {
    const release = app.releases[channel];
    return release ? [{ channel, release }] : [];
  }));
  const updates = $derived(latest.filter(({ release }) => release.action.kind === 'update'));
  const directUpdate = $derived(updates.length === 1 && updates[0].channel === 'stable' ? updates[0].release : undefined);

  const choices = $derived.by(() => {
    const seen = new Set<string>();
    return [...latest, ...archived].filter(({ channel, release }) => {
      const key = `${channel}:${release.version}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  });

  async function showPanel(next: NonNullable<typeof panel>) {
    panel = next;
    await tick();
    if (!dialog.open) dialog.showModal();
    else dialog.querySelector('h3')?.focus();
  }

  function handleDialogClose() {
    panel = undefined;
    confirmUninstall = undefined;
    replacement = undefined;
  }

  function chooseRelease(channel: Channel, release: Release, graphical = false) {
    if (release.action.kind === 'replace') {
      replacement = { channel, version: release.version, key: release.action.key, graphical, fromCatalog: panel === 'catalog' };
      void showPanel('replace');
    } else {
      if (dialog.open) dialog.close();
      onInstall(channel, release, graphical);
    }
  }

  function openCatalog() {
    archived = [];
    nextVersion = null;
    incompleteVersions = false;
    void showPanel('catalog');
    void loadVersions();
  }

  function update() {
    if (directUpdate) chooseRelease('stable', directUpdate);
    else if (updates.length) void showPanel('updates');
  }

  async function loadVersions() {
    const current = ++catalogRequest;
    loadingVersions = true;
    versionError = '';
    try {
      const page = await launcher.versions(app.id, nextVersion);
      if (current !== catalogRequest) return;
      archived = [...archived, ...page.versions];
      nextVersion = page.next;
      incompleteVersions ||= page.incomplete;
    } catch (reason) {
      if (current === catalogRequest) versionError = reason instanceof Error ? reason.message : String(reason);
    } finally {
      if (current === catalogRequest) loadingVersions = false;
    }
  }

  function retrying(channel: Channel, release: Release) {
    return outcome?.retryFor?.channel === channel && outcome.retryFor.version === release.version;
  }
</script>

<article class="app-detail" aria-labelledby="title-{app.id}">
  <div class="app-title">
    <h2 id="title-{app.id}" tabindex="-1">{app.name}</h2>
    <p>{app.description}</p>
  </div>

  <div class="version-row">
    {#if app.installed}
      <span class="version" title="Verified installed version"><i></i> {app.installed.version} installed</span>
    {/if}
    {#if app.stale}<span class="version cached">Cached</span>{/if}
  </div>

  <div class="detail-links" class:with-update={canOpen && updates.length > 0}>
    {#if canOpen && app.installed}
      {@const installed = app.installed}
      <div class="launch-actions">
        <button class="button" class:primary={!updates.length} disabled={!!openingVersion} onclick={() => onOpen(installed)}>
          {openingVersion === installed.key ? 'Opening…' : 'Open'}
        </button>
        {#if updates.length}
          <button class="button primary" disabled={installDisabled} aria-haspopup={directUpdate ? undefined : 'dialog'} onclick={update}>
            Update
          </button>
        {/if}
      </div>
    {/if}
    {#if app.installedVersions.length}
      <button class="text-button" aria-haspopup="dialog" onclick={() => showPanel('versions')}>
        Manage versions{app.installedVersions.length > 1 ? ` (${app.installedVersions.length})` : ''}
      </button>
    {/if}
    <button class="text-button" aria-haspopup="dialog" onclick={openCatalog}>
      {app.installed ? 'Choose version…' : 'All versions…'}
    </button>
  </div>

  {#if progress}
    <div class="operation">
      <div class="operation-progress">
        <div class="operation-label">
          <span role="status">{progress.stage === 'downloading' && canceling ? 'Cancelling…' : stages[progress.stage]}</span>
          {#if progress.stage === 'downloading' && percent !== undefined}<span>{percent}%</span>{/if}
        </div>
        <progress aria-label={stages[progress.stage]} max="100" value={progress.stage === 'downloading' ? percent : undefined}></progress>
      </div>
      {#if progress.stage === 'downloading'}
        <button class="button" disabled={canceling} onclick={onCancel}>Cancel</button>
      {/if}
    </div>
  {/if}

  {#if !app.installed}
  <div class="release-choices" role="group" aria-label="Release channels">
    {#each channels as channel}
      {@const release = app.releases[channel]}
      {@const name = channelName[channel]}
      <section class="release-choice" aria-label={name}>
        <div class="release-heading">
          <h3>{name}</h3>
          {#if release}<span class="release-version">{release.version}</span>{/if}
        </div>
        {#if release}
          {@const action = release.action.kind}
          {@const retry = retrying(channel, release)}
          {#if action === 'installed' || action === 'unavailable'}
            <span class="release-status">{actions[action]}</span>
          {:else if !progress}
            <button
              class="button release-action"
              aria-label={`${retry ? 'Retry' : actions[action]} ${name} ${release.version}`}
              disabled={installDisabled || outcome?.kind === 'restartRequired'}
              onclick={() => chooseRelease(channel, release)}
            >{retry ? 'Retry' : actions[action]}</button>
            {#if retry && outcome?.retryGraphically}
              <button class="text-button" disabled={installDisabled} aria-label={`Retry ${name} with installer`} onclick={() => chooseRelease(channel, release, true)}>
                With installer
              </button>
            {/if}
          {/if}
        {:else}
          <span class="release-status">{missingBuild}</span>
        {/if}
      </section>
    {/each}
  </div>
  {/if}

  {#if !progress && outcome}
    <p class:failed={outcome.kind === 'failed'} class="outcome" role={outcome.kind === 'failed' ? 'alert' : 'status'}>{outcome.message}</p>
  {/if}

  <dialog class="detail-dialog" aria-labelledby="panel-{app.id}" bind:this={dialog} onclose={handleDialogClose}>
    <header>
      <div>
        <p>{app.name}</p>
        <h3 id="panel-{app.id}" tabindex="-1">{panel === 'versions' ? 'Manage versions' : panel === 'replace' ? 'Replace installed version' : panel === 'updates' ? 'Choose an update' : 'Choose a version'}</h3>
      </div>
      <button class="button dialog-close" aria-label="Close" onclick={() => dialog.close()}>×</button>
    </header>

    {#if panel === 'versions'}
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
                dialog.close();
                onUninstall(version);
              }}>Uninstall version</button>
              <button class="button" onclick={() => { confirmUninstall = undefined; }}>Keep installed</button>
            </div>
          {:else}
            <div class="version-controls">
              <button class="button" disabled={!!progress || !!openingVersion} onclick={() => {
                dialog.close();
                onOpen(version);
              }}>
                {openingVersion === version.key ? 'Opening…' : 'Open'}
              </button>
              {#if version.canRetain}
                <button class="button" disabled={installDisabled} onclick={() => {
                  dialog.close();
                  onKeepCopy(version);
                }}>Keep a copy</button>
              {/if}
              {#if version.canUninstall}
                <button class="button" disabled={installDisabled} onclick={() => { confirmUninstall = version.key; }}>Uninstall…</button>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    {:else if panel === 'replace' && replacement}
      {@const choice = replacement}
      {@const release = choices.find((entry) => entry.channel === choice.channel && entry.release.version === choice.version)?.release}
      {#if release?.version === choice.version && release.action.kind === 'replace' && release.action.key === choice.key}
        <p>Replace {app.name} {release.action.version} with {channelName[choice.channel]} {release.version}?</p>
        <div class="version-controls">
          <button class="button" onclick={() => {
            if (choice.fromCatalog) void showPanel('catalog');
            else dialog.close();
          }}>{choice.fromCatalog ? 'Back' : 'Cancel'}</button>
          <button class="button danger" disabled={installDisabled} onclick={() => {
            dialog.close();
            onInstall(choice.channel, release, choice.graphical);
          }}>Replace installed version</button>
        </div>
      {:else}
        <p>The available or installed version changed. Choose the release again.</p>
      {/if}
    {:else if panel === 'catalog' || panel === 'updates'}
      {#each (panel === 'updates' ? updates : choices) as { channel, release } (`${channel}:${release.version}`)}
        {@const action = release.action.kind}
        {@const name = channelName[channel]}
        {@const retry = retrying(channel, release)}
        <div class="published-version">
          <div class="published-version-row">
            <div class="installed-version-label"><strong>{release.version}</strong><span>{name}</span></div>
            {#if action === 'installed' || action === 'unavailable'}
              <span class="release-status">{actions[action]}</span>
            {:else}
              <button class="button" aria-label={`${retry ? 'Retry' : actions[action]} ${name} ${release.version}`}
                disabled={installDisabled || outcome?.kind === 'restartRequired'} onclick={() => chooseRelease(channel, release)}>
                {retry ? 'Retry' : actions[action]}
              </button>
              {#if retry && outcome?.retryGraphically}
                <button class="text-button" disabled={installDisabled} onclick={() => chooseRelease(channel, release, true)}>With installer</button>
              {/if}
            {/if}
          </div>
          {#if release.notes.length}
            <details>
              <summary>Release notes</summary>
              <ul class="notes">{#each release.notes as note}<li>{note}</li>{/each}</ul>
            </details>
          {/if}
        </div>
      {/each}
      {#if panel === 'updates'}
        {#if !updates.length}<p>No updates available.</p>{/if}
      {:else if loadingVersions}
        <p role="status">Checking available versions…</p>
      {:else if versionError}
        <p class="refresh-error" role="alert">{versionError}</p>
        <button class="button" onclick={openCatalog}>Reload versions</button>
      {:else}
        {#if !choices.length}<p>{missingBuild}</p>{/if}
        {#if nextVersion}<button class="button version-more" onclick={loadVersions}>More versions</button>{/if}
        {#if incompleteVersions}<p role="status">Some versions could not be checked.</p>{/if}
      {/if}
      {#if app.refreshError}
        <p class="refresh-error">{app.refreshError}</p>
      {/if}
    {/if}
  </dialog>
</article>
