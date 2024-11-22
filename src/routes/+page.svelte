<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { createMemoryClient, type EIP1193RequestFn, type MemoryClient } from 'tevm';
	import { type PublicClient, custom, createPublicClient } from 'viem';

	let startMessage = $state<string>();

	let tevmClient = $state<MemoryClient | undefined>(undefined);
	let client = $state<PublicClient>(
		createPublicClient({
			transport: custom({
				request: (request) => {
					return invoke('request', request) as any;
				}
			})
		})
	);

	let block = $state<any>();

	const start = async () => {
		startMessage = await invoke('start');
	};

	const fork = () => {
		tevmClient = createMemoryClient({
			fork: {
				transport: client
			}
		});
	};
  const unfork = () => {
    tevmClient = undefined
  }

	const getBlock = () => {
		client.getBlock({blockTag: 'latest'}).then(latestBlock => {
      block = latestBlock
    })
	};

	$effect(() => {
		if (!startMessage) return;
		const interval = setInterval(getBlock, 2000);

		// Clean up effect
		return () => {
			clearInterval(interval);
		};
	});
</script>

<h1>Mana</h1>
<button onclick={start}>Start</button>
<button onclick={tevmClient ? unfork : fork}>{tevmClient ? 'Unfork' : 'Fork'}</button>
{#if startMessage}
	<p>{startMessage}</p>
{/if}
{#if block}
	<pre>{JSON.stringify(block, null, 2)}</pre>
{/if}

<p>Tauri <a href="https://v1.tauri.app/v1/guides/getting-started/setup/sveltekit">docs</a></p>
<p>Sveltekit <a href="https://svelte.dev/docs/kit">docs</a></p>
