<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  type Product = { id: string; name: string; description: string };
  let products = $state<Product[]>([]);
  let error = $state('');

  onMount(() => {
    invoke<Product[]>('catalog').then((result) => (products = result)).catch((reason) => (error = String(reason)));
  });
</script>

<main>
  <h1>Golden Launcher</h1>
  {#if error}<p role="alert">{error}</p>{/if}
  <ul>
    {#each products as product (product.id)}
      <li><strong>{product.name}</strong> · {product.description}</li>
    {/each}
  </ul>
</main>
