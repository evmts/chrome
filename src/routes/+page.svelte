<script lang="ts">
	import { invoke } from "@tauri-apps/api/core";

  let startMessage = $state<string>()

  const start = async () => {
    startMessage = await invoke('start')
  }
</script>

<h1>Mana</h1>
<button onclick={start}>Start</button>
{#if startMessage}
  <p>{startMessage}</p>
{/if}
<p>Tauri <a href="https://v1.tauri.app/v1/guides/getting-started/setup/sveltekit">docs</a></p>
<p>Sveltekit <a href="https://svelte.dev/docs/kit">docs</a></p>
