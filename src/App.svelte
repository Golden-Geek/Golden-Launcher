<script lang="ts">
  import { onMount, tick } from 'svelte';
  import AppCard from './AppCard.svelte';
  import { launcher, type AppId, type CardOutcome, type Channel, type InstallProgress, type LauncherApp } from './api';
  import './styles.css';

  let apps = $state<LauncherApp[]>([]);
  let view = $state<'library' | 'updates'>('library');
  let isRefreshing = $state(false);
  let isLoading = $state(true);
  let error = $state('');
  let lastChecked = $state<Date>();
  let operation = $state<InstallProgress>();
  let canceling = $state(false);
  let outcomes = $state<Partial<Record<AppId, CardOutcome>>>({});
  let settingsPending = $state<Partial<Record<AppId, boolean>>>({});
  let opening = $state<Partial<Record<AppId, boolean>>>({});
  let updateNotice = $state('');
  let heading: HTMLHeadingElement;
  let request = 0;

  const updates = $derived(apps.filter((app) => app.updateAvailable));
  const visible = $derived(view === 'library' ? apps : updates);
  const staleCount = $derived(apps.filter((app) => app.stale).length);
  const failedCount = $derived(apps.filter((app) => app.refreshError).length);
  const installedCount = $derived(apps.filter((app) => app.installed).length);

  function message(reason: unknown) {
    return reason instanceof Error ? reason.message : String(reason);
  }

  async function loadLibrary() {
    const current = ++request;
    try {
      const next = await launcher.library();
      if (current === request) apps = next;
    } catch (reason) {
      if (current === request) error = message(reason);
    }
  }

  async function refresh() {
    if (isRefreshing) return;
    isRefreshing = true;
    error = '';
    try {
      const refreshed = await launcher.refresh();
      if (refreshed.some((app) => !app.refreshError)) lastChecked = new Date();
    } catch (reason) {
      error = message(reason);
    } finally {
      await loadLibrary();
      isRefreshing = false;
    }
  }

  onMount(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    void launcher.onProgress((progress) => {
      if (operation?.id === progress.id) operation = progress;
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unsubscribe = unlisten;
    }).catch((reason) => {
      error = message(reason);
    });
    void loadLibrary().then(() => {
      isLoading = false;
      if (!disposed && apps.length) void refresh();
    });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  });

  async function setView(next: typeof view) {
    if (view === next) return;
    view = next;
    updateNotice = '';
    await tick();
    heading?.focus();
  }

  async function chooseChannel(app: LauncherApp, channel: Channel) {
    settingsPending[app.id] = true;
    delete outcomes[app.id];
    try {
      await launcher.setChannel(app.id, channel);
      await loadLibrary();
    } catch (reason) {
      outcomes[app.id] = { kind: 'failed', message: message(reason), retryGraphically: false };
    } finally {
      settingsPending[app.id] = false;
    }
  }

  async function install(app: LauncherApp, graphical = false) {
    if (!app.available || operation) return;
    updateNotice = '';
    delete outcomes[app.id];
    operation = { id: app.id, stage: 'downloading', downloaded: 0, total: null };
    try {
      const result = await launcher.install(app.id, app.available.version, graphical);
      outcomes[app.id] = { ...result, retryInstall: result.kind === 'failed' || result.kind === 'cancelled' };
      if (view === 'updates' && result.kind === 'installed') {
        updateNotice = `${app.name} updated to ${app.available.version}.`;
      }
    } catch (reason) {
      outcomes[app.id] = { kind: 'failed', message: message(reason), retryGraphically: false, retryInstall: true };
    } finally {
      await loadLibrary();
      canceling = false;
      operation = undefined;
    }
  }

  async function cancel(id: AppId) {
    canceling = true;
    try {
      if (!await launcher.cancel(id)) canceling = false;
    } catch (reason) {
      canceling = false;
      error = message(reason);
    }
  }

  async function uninstall(app: LauncherApp) {
    if (!app.installed || operation) return;
    updateNotice = '';
    delete outcomes[app.id];
    operation = { id: app.id, stage: 'uninstalling', downloaded: 0, total: null };
    try {
      const result = await launcher.uninstall(app.id, app.installed.version);
      outcomes[app.id] = result;
      if (view === 'updates' && result.kind === 'uninstalled') updateNotice = `${app.name} was uninstalled.`;
    } catch (reason) {
      outcomes[app.id] = { kind: 'failed', message: message(reason), retryGraphically: false };
    } finally {
      await loadLibrary();
      operation = undefined;
    }
  }

  async function open(app: LauncherApp) {
    opening[app.id] = true;
    try {
      await launcher.open(app.id);
    } catch (reason) {
      outcomes[app.id] = { kind: 'failed', message: message(reason), retryGraphically: false };
      await loadLibrary();
    } finally {
      opening[app.id] = false;
    }
  }
</script>

<div class="shell">
  <aside class="sidebar">
    <div class="brand">
      <img src="/launcher.png" alt="" width="42" height="42" />
      <span>Golden<strong>Launcher</strong></span>
    </div>

    <nav aria-label="Launcher">
      <button class:active={view === 'library'} aria-current={view === 'library' ? 'page' : undefined} onclick={() => setView('library')}>
        <span class="nav-icon grid-icon" aria-hidden="true"></span> Library
      </button>
      <button class:active={view === 'updates'} aria-current={view === 'updates' ? 'page' : undefined} onclick={() => setView('updates')}>
        <span class="nav-icon updates-icon" aria-hidden="true">↓</span> Updates
        {#if updates.length}<span class="nav-count">{updates.length}</span>{/if}
      </button>
    </nav>

    <div class="connection" aria-live="polite">
      <span class:healthy={!!lastChecked && !failedCount && !isRefreshing} class:stale={!!staleCount}></span>
      {#if isRefreshing}
        Checking releases…
      {:else if staleCount}
        Cached release data
      {:else if failedCount}
        Release check incomplete
      {:else if lastChecked}
        Checked at {lastChecked.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
      {:else}
        Release status pending
      {/if}
    </div>
  </aside>

  <main>
    <header class="page-heading">
      <div>
        <h1 tabindex="-1" bind:this={heading}>{view === 'library' ? 'Library' : 'Updates'}</h1>
        {#if view === 'library'}
          <p>{apps.length} apps <span aria-hidden="true">·</span> {installedCount} installed</p>
        {:else}
          <p>{updates.length} {updates.length === 1 ? 'update' : 'updates'} available</p>
        {/if}
      </div>
      <button class="button refresh" disabled={isRefreshing || isLoading} onclick={refresh}>
        <span class:spinning={isRefreshing} aria-hidden="true">↻</span>
        {isRefreshing ? 'Checking…' : 'Refresh'}
      </button>
    </header>

    {#if error}
      <p class="banner banner-error" role="alert">{error}</p>
    {:else if failedCount}
      <p class="banner" role="status">
        Release data could not be refreshed for {failedCount} {failedCount === 1 ? 'app' : 'apps'}.
        {#if staleCount} Showing cached releases where available.{/if}
      </p>
    {/if}
    {#if updateNotice}<p class="banner banner-success" role="status">{updateNotice}</p>{/if}

    {#if isLoading}
      <div class="empty" role="status"><p>Loading your library…</p></div>
    {:else if visible.length}
      <div class="cards" aria-label={view === 'library' ? 'Apps' : 'Available updates'}>
        {#each visible as app (app.id)}
          <AppCard
            {app}
            progress={operation?.id === app.id ? operation : undefined}
            {canceling}
            outcome={outcomes[app.id]}
            installDisabled={!!operation || !!settingsPending[app.id]}
            settingsDisabled={operation?.id === app.id || !!settingsPending[app.id]}
            isOpening={!!opening[app.id]}
            onChannel={(channel) => chooseChannel(app, channel)}
            onInstall={(graphical) => install(app, graphical)}
            onCancel={() => cancel(app.id)}
            onOpen={() => open(app)}
            onUninstall={() => uninstall(app)}
          />
        {/each}
      </div>
    {:else if view === 'updates'}
      <div class="empty">
        <span class="empty-mark" aria-hidden="true">✓</span>
        <h2>{isRefreshing ? 'Checking for updates' : 'No updates available'}</h2>
        <p>Available updates for installed apps appear here.</p>
        <button class="button" onclick={() => setView('library')}>View library</button>
      </div>
    {:else}
      <div class="empty"><p>The library could not be loaded.</p></div>
    {/if}
  </main>
</div>
