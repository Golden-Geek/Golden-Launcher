<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { AppId, LauncherApp } from './api';

  interface Props {
    apps: LauncherApp[];
    selectedId?: AppId;
    busyId?: AppId;
    onSelect: (id: AppId, fromKeyboard: boolean) => void;
    children: Snippet;
  }

  let { apps, selectedId, busyId, onSelect, children }: Props = $props();
  let pointer = $state<{ x: number; y: number; size: number }>();
  let backdrop: HTMLDivElement;

  const zoneDefinitions = [
    { id: 'installed', label: 'Installed' },
    { id: 'update', label: 'Update available' },
    { id: 'notInstalled', label: 'Not installed' },
  ] as const;
  const trackRadius = .27;
  const iconRadius = .386;
  const halfZoneGap = .8;

  function point(angle: number, radius: number) {
    const radians = angle * Math.PI / 180;
    return { x: .5 + radius * Math.sin(radians), y: .5 - radius * Math.cos(radians) };
  }

  function captionPath(angle: number) {
    const reverse = Math.cos(angle * Math.PI / 180) < 0;
    const captionRadius = trackRadius + (reverse ? .018 : .003);
    const halfArc = reverse ? -32 : 32;
    const start = point(angle - halfArc, captionRadius);
    const end = point(angle + halfArc, captionRadius);
    const radius = captionRadius * 1000;
    return `M ${start.x * 1000} ${start.y * 1000} A ${radius} ${radius} 0 0 ${reverse ? 0 : 1} ${end.x * 1000} ${end.y * 1000}`;
  }

  const layout = $derived.by(() => {
    const groups: Record<(typeof zoneDefinitions)[number]['id'], LauncherApp[]> = {
      installed: [],
      update: [],
      notInstalled: [],
    };
    for (const app of apps) {
      const { stable, beta } = app.releases;
      const hasUpdate = stable?.action.kind === 'update' || beta?.action.kind === 'update';
      const zone = hasUpdate ? 'update' : app.installed ? 'installed' : 'notInstalled';
      groups[zone].push(app);
    }

    const presentZones = zoneDefinitions.filter(({ id }) => groups[id].length);
    const singleZone = presentZones.length === 1;
    const appSpace = 360 / Math.max(1, apps.length);
    let angle = -appSpace / 2;
    const zones = presentZones.map(({ id, label }) => {
      const edge = angle;
      const group = groups[id];
      const span = group.length * appSpace;
      const start = edge + halfZoneGap;
      const end = edge + span - halfZoneGap;
      const sweep = end - start;
      angle += span;
      return {
        id,
        label,
        edges: singleZone ? [] : [start, end],
        path: captionPath(singleZone ? 0 : edge + span / 2),
        captionLength: span <= 45 && label.length > 10 ? (span - 6) * Math.PI / 180 * trackRadius * 1000 : undefined,
        mask: singleZone ? 'none' : `conic-gradient(from ${start}deg, white 0deg ${sweep}deg, transparent ${sweep}deg 360deg)`,
        slots: group.map((app, index) => ({ app, label, ...point(edge + (index + .5) * appSpace, iconRadius) })),
      };
    });

    return { zones, slots: zones.flatMap((zone) => zone.slots) };
  });

  function movePointer(event: PointerEvent & { currentTarget: HTMLDivElement }) {
    if (event.pointerType === 'touch') return;
    const rect = event.currentTarget.getBoundingClientRect();
    pointer = { x: event.clientX - rect.left, y: event.clientY - rect.top, size: rect.width };
  }

  function tiltBackdrop(event: PointerEvent) {
    if (event.pointerType === 'touch') return;
    const halfWidth = window.innerWidth / 2;
    const halfHeight = window.innerHeight / 2;
    const x = (event.clientX - halfWidth) / Math.min(halfWidth, 500);
    const y = (event.clientY - halfHeight) / Math.min(halfHeight, 400);
    backdrop?.style.setProperty('--grid-x', String(Math.max(-1, Math.min(x, 1))));
    backdrop?.style.setProperty('--grid-y', String(Math.max(-1, Math.min(y, 1))));
  }

  function resetBackdrop() {
    backdrop?.style.removeProperty('--grid-x');
    backdrop?.style.removeProperty('--grid-y');
  }

  function attraction(x: number, y: number, selected: boolean) {
    if (!pointer || selected) return { x: 0, y: 0, strength: 0 };
    const dx = pointer.x - x * pointer.size;
    const dy = pointer.y - y * pointer.size;
    const reach = Math.min(180, pointer.size * .27);
    const strength = Math.max(0, 1 - Math.hypot(dx, dy) / reach) ** 2;
    return { x: dx * strength * .44, y: dy * strength * .44, strength };
  }

  function status(app: LauncherApp) {
    if (busyId === app.id) return 'In progress';
    const { stable, beta } = app.releases;
    if (beta?.action.kind === 'update' && stable?.action.kind !== 'update') return 'Beta update';
    if (!stable && beta) return 'Beta only';
    if (app.availability === 'awaitingRelease') return 'Awaiting release';
  }
</script>

<svelte:window onpointermove={tiltBackdrop} onpointerout={(event) => { if (!event.relatedTarget) resetBackdrop(); }} onblur={resetBackdrop} />

<div
  class="orbit"
  role="group"
  aria-label="Apps"
  onpointermove={movePointer}
  onpointerleave={() => { pointer = undefined; }}
  onpointercancel={() => { pointer = undefined; }}
>
  <div class="orbit-backdrop" bind:this={backdrop} aria-hidden="true">
    <img class="orbit-logo" class:visible={!selectedId && apps.length > 0} src="/launcher.png" alt="" draggable="false" />
  </div>
  {#if layout.zones.length}
    <div class="orbit-dial" style:--ring-edge={`${trackRadius * 200}%`} aria-hidden="true">
      {#each layout.zones as { id, edges, mask } (id)}
        <div class="orbit-zone-band zone-{id}" style:mask-image={mask}></div>
        {#each edges as edge (edge)}
          <div class="orbit-zone-edge zone-{id}" style:--edge-angle={`${edge}deg`}></div>
        {/each}
      {/each}
    </div>
    <svg class="orbit-zone-labels" viewBox="0 0 1000 1000" aria-hidden="true">
      <defs>
        {#each layout.zones as { id, path } (id)}
          <path id="orbit-zone-{id}" d={path} />
        {/each}
      </defs>
      {#each layout.zones as { id, label, captionLength } (id)}
        <text class="orbit-zone zone-{id}" text-anchor="middle" textLength={captionLength} lengthAdjust="spacingAndGlyphs">
          <textPath href="#orbit-zone-{id}" startOffset="50%">{label}</textPath>
        </text>
      {/each}
    </svg>
  {/if}
  {#each layout.slots as { app, label, x, y } (app.id)}
    {@const selected = selectedId === app.id}
    {@const motion = attraction(x, y, selected)}
    {@const chip = status(app)}
    <button
      id="orbit-{app.id}"
      type="button"
      class="orbit-app"
      class:selected
      class:label-above={y < .32}
      aria-label={`${app.name}, ${chip ? `${chip}, ` : ''}${label}`}
      aria-expanded={selected}
      aria-controls="app-focus"
      title={selected ? 'Return to orbit' : undefined}
      style:--slot-x={`${x * 100}%`}
      style:--slot-y={`${y * 100}%`}
      style:--pull-x={`${motion.x}px`}
      style:--pull-y={`${motion.y}px`}
      style:--zoom={1 + motion.strength * .32}
      style:--icon-depth={1 + Math.round(motion.strength * 3)}
      onclick={(event) => onSelect(app.id, event.detail === 0)}
    >
      <img class="orbit-icon" src="/apps/{app.id}.png" alt="" draggable="false" />
      <span class="orbit-label" aria-hidden="true">
        <span class="orbit-name">{app.name}</span>
        {#if chip}
          <span class="orbit-chip">{chip}</span>
        {/if}
      </span>
    </button>
  {/each}

  <div id="app-focus" class="orbit-hub" class:focused={!!selectedId}>
    {@render children()}
  </div>
</div>
