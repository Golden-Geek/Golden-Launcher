<script lang="ts">
  import { onMount, tick } from 'svelte';
  import AppCard from './AppCard.svelte';
  import AppOrbit from './AppOrbit.svelte';
  import { launcher, selfUpdater, type AppId, type CardOutcome, type Channel, type InstalledVersion, type InstallProgress, type LauncherApp, type Release } from './api';
  import './styles.css';

  let apps = $state<LauncherApp[]>([]);
  let selectedId = $state<AppId>();
  let isRefreshing = $state(false);
  let isLoading = $state(true);
  let error = $state('');
  let operation = $state<InstallProgress>();
  let canceling = $state(false);
  let outcomes = $state<Partial<Record<AppId, CardOutcome>>>({});
  let opening = $state<Partial<Record<AppId, string>>>({});
  let launcherVersion = $state<string | null>(null);
  let launcherUpdateError = $state('');
  let launcherChecking = $state(false);
  let launcherInstalling = $state(false);
  let request = 0;

  const selected = $derived(apps.find((app) => app.id === selectedId));
  const staleCount = $derived(apps.filter((app) => app.stale).length);
  const failedCount = $derived(apps.filter((app) => app.refreshError).length);

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
      await launcher.refresh();
    } catch (reason) {
      error = message(reason);
    } finally {
      await loadLibrary();
      isRefreshing = false;
    }
  }

  async function checkLauncherUpdate() {
    if (launcherChecking || launcherVersion || launcherInstalling) return;
    launcherChecking = true;
    launcherUpdateError = '';
    try {
      launcherVersion = await selfUpdater.prepare();
    } catch (reason) {
      console.warn('Could not check for a Golden Launcher update.', reason);
    } finally {
      launcherChecking = false;
    }
  }

  async function installLauncherUpdate() {
    if (!launcherVersion || launcherInstalling || operation) return;
    launcherInstalling = true;
    launcherUpdateError = '';
    try {
      await selfUpdater.install();
    } catch (reason) {
      launcherUpdateError = message(reason);
      launcherInstalling = false;
    }
  }

  function refreshAll() {
    void refresh();
    void checkLauncherUpdate();
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
    void checkLauncherUpdate();
    const updateTimer = window.setInterval(checkLauncherUpdate, 4 * 60 * 60 * 1000);
    return () => {
      disposed = true;
      unsubscribe?.();
      window.clearInterval(updateTimer);
    };
  });

  async function focusAppDetails(id: AppId) {
    await tick();
    document.getElementById(`title-${id}`)?.focus();
  }

  async function focusOrbitApp(id: AppId) {
    await tick();
    document.getElementById(`orbit-${id}`)?.focus();
  }

  function selectApp(id: AppId, fromKeyboard: boolean) {
    selectedId = selectedId === id ? undefined : id;
    if (fromKeyboard) {
      if (selectedId) void focusAppDetails(id);
      else void focusOrbitApp(id);
    }
  }

  async function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'F5') {
      event.preventDefault();
      if (!isLoading) refreshAll();
      return;
    }
    if (event.key !== 'Escape' || !selected || document.querySelector('dialog[open]')) return;
    event.preventDefault();
    const id = selected.id;
    selectedId = undefined;
    await focusOrbitApp(id);
  }

  async function install(app: LauncherApp, channel: Channel, release: Release, graphical = false) {
    if (operation || launcherInstalling) return;
    delete outcomes[app.id];
    operation = { id: app.id, stage: 'downloading', downloaded: 0, total: null };
    const retryFor = { channel, version: release.version };
    try {
      const result = await launcher.install(app.id, channel, release, graphical);
      outcomes[app.id] = {
        ...result,
        retryFor: result.kind === 'failed' || result.kind === 'cancelled' ? retryFor : undefined,
      };
    } catch (reason) {
      outcomes[app.id] = { kind: 'failed', message: message(reason), retryGraphically: false, retryFor };
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

  async function manage(app: LauncherApp, version: InstalledVersion, action: 'uninstall' | 'keepCopy') {
    if (operation || launcherInstalling) return;
    delete outcomes[app.id];
    const stage = action === 'uninstall' ? 'uninstalling' : 'retaining';
    operation = { id: app.id, stage, downloaded: 0, total: null };
    try {
      const result = await launcher[action](app.id, version.key);
      outcomes[app.id] = result;
    } catch (reason) {
      outcomes[app.id] = { kind: 'failed', message: message(reason), retryGraphically: false };
    } finally {
      await loadLibrary();
      operation = undefined;
    }
  }

  async function open(app: LauncherApp, version: InstalledVersion) {
    opening[app.id] = version.key;
    try {
      await launcher.open(app.id, version.key);
    } catch (reason) {
      outcomes[app.id] = { kind: 'failed', message: message(reason), retryGraphically: false };
      await loadLibrary();
    } finally {
      delete opening[app.id];
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<main class="workspace" aria-label="Golden Launcher">
  {#if error || failedCount || launcherVersion}
    <div class="notices">
      {#if error || failedCount}
        <div class="banner notice-actions" class:banner-error={!!error}>
          {#if error}
            <p role="alert">{error}</p>
          {:else}
            <p role="status">
              Release data could not be refreshed for {failedCount} {failedCount === 1 ? 'app' : 'apps'}.
              {#if staleCount} Showing cached releases where available.{/if}
            </p>
          {/if}
          <button class="button" disabled={isRefreshing || isLoading} onclick={refreshAll}>
            {isRefreshing ? 'Checking…' : 'Retry'}
          </button>
        </div>
      {/if}
      {#if launcherVersion}
        <div class="banner notice-actions">
          <div>
            <p role="status">Golden Launcher {launcherVersion} is ready to install.</p>
            {#if launcherUpdateError}<p role="alert">Launcher update failed. {launcherUpdateError}</p>{/if}
          </div>
          <button class="button primary" disabled={launcherInstalling || !!operation} onclick={installLauncherUpdate}>
            {launcherInstalling ? 'Installing…' : 'Install and restart'}
          </button>
        </div>
      {/if}
    </div>
  {/if}

  <AppOrbit {apps} selectedId={selected?.id} busyId={operation?.id} onSelect={selectApp}>
    {#if isLoading}
      <p class="hub-subtitle" role="status">Loading your library…</p>
    {:else if selected}
      {@const app = selected}
      {#key app.id}
        <AppCard
          {app}
          progress={operation?.id === app.id ? operation : undefined}
          {canceling}
          outcome={outcomes[app.id]}
          installDisabled={!!operation || launcherInstalling}
          openingVersion={opening[app.id]}
          onInstall={(channel, release, graphical) => install(app, channel, release, graphical)}
          onCancel={() => cancel(app.id)}
          onOpen={(version) => open(app, version)}
          onUninstall={(version) => manage(app, version, 'uninstall')}
          onKeepCopy={(version) => manage(app, version, 'keepCopy')}
        />
      {/key}
    {:else if !apps.length && !error}
      <p class="hub-subtitle">The library could not be loaded.</p>
    {/if}
  </AppOrbit>
</main>
