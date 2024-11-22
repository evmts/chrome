<script lang="ts">
	import { invoke } from "@tauri-apps/api/core";

  let startMessage = $state<string>()

  let block = $state<any>()

  const start = async () => {
    startMessage = await invoke('start')
  }

  const getBlock = async () => {
    block = await invoke('get_block')
  }

  $effect(() => {
    if (!startMessage) return
    const interval = setInterval(getBlock, 2000)

    // Clean up effect
    return () => {
      clearInterval(interval)
    }
  })


</script>

<h1>Mana</h1>
<button onclick={start}>Start</button>
{#if startMessage}
  <p>{startMessage}</p>
{/if}
{#if block}
  <pre>{JSON.stringify(block, null, 2)}</pre>
{/if}

<p>Tauri <a href="https://v1.tauri.app/v1/guides/getting-started/setup/sveltekit">docs</a></p>
<p>Sveltekit <a href="https://svelte.dev/docs/kit">docs</a></p>
