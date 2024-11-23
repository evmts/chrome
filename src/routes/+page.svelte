<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { createMemoryClient, type EIP1193RequestFn, type JsonRpcResponse, type MemoryClient } from 'tevm';
	import { type PublicClient, custom, createPublicClient } from 'viem';
	import { 
		PUBLIC_EXECUTION_RPC,
		PUBLIC_CONSENSUS_RPC 
	} from '$env/static/public';

	let startMessage = $state<string>();
	let rpcUrl = $state<string>(PUBLIC_EXECUTION_RPC);
	const CONSENSUS_RPC = PUBLIC_CONSENSUS_RPC;

	let tevmClient = $state<MemoryClient | undefined>(undefined);
	let client = $state<PublicClient>(
		createPublicClient({
				transport: custom({
					request: (request) => {
						return invoke('request', {request:
							{
								...request,
								jsonrpc: '2.0',
								id: crypto.randomUUID(),
							}
						}).then((response: any) => {
							if (response.error) throw response.error
							return response.result
						}).catch(e => {
							throw e
						}) as any;
					}
				})
		})
	);

	let block = $state<any>();

	const start = async () => {
		startMessage = await invoke('start', {
			rpcUrl: rpcUrl,
			consensusRpc: CONSENSUS_RPC,
			chainId: 1
		}).catch(e => {
			return e as string
		}) as string;
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
		return client.getBlock({blockTag: 'latest'}).then(latestBlock => {
      block = latestBlock
    })
	};

	$effect(() => {
		if (!startMessage) return;

		let timeoutId: NodeJS.Timeout;
		let isRunning = true
		const pollBlock = async () => {
			await getBlock();
			if (!isRunning) return
			timeoutId = setTimeout(pollBlock, 10_000);
		};

		pollBlock();

		// Clean up effect
		return () => {
			isRunning = false
			clearTimeout(timeoutId);
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
	<table>
		<thead>
			<tr>
				<th>Property</th>
				<th>Value</th>
			</tr>
		</thead>
		<tbody>
			<tr>
				<td>Hash</td>
				<td>{block.hash}</td>
			</tr>
			<tr>
				<td>Number</td>
				<td>{block.number}</td>
			</tr>
			<tr>
				<td>Parent Hash</td>
				<td>{block.parentHash}</td>
			</tr>
			<tr>
				<td>Timestamp</td>
				<td>{new Date(Number(block.timestamp) * 1000).toLocaleString()}</td>
			</tr>
			<tr>
				<td>Gas Used</td>
				<td>{block.gasUsed.toString()}</td>
			</tr>
			<tr>
				<td>Gas Limit</td>
				<td>{block.gasLimit.toString()}</td>
			</tr>
			<tr>
				<td>Base Fee</td>
				<td>{block.baseFeePerGas?.toString() ?? 'N/A'}</td>
			</tr>
			<tr>
				<td>Transactions</td>
				<td>{block.transactions.length} transactions</td>
			</tr>
		</tbody>
	</table>
{/if}

<p>Tauri <a href="https://v1.tauri.app/v1/guides/getting-started/setup/sveltekit">docs</a></p>
<p>Sveltekit <a href="https://svelte.dev/docs/kit">docs</a></p>
