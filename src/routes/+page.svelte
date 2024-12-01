<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { createMemoryClient, type EIP1193RequestFn, type JsonRpcResponse, type MemoryClient } from 'tevm';
	import { type PublicClient, custom, createPublicClient } from 'viem';
	import { whatsabi } from '@shazow/whatsabi';
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
	let contractAddress = $state<string>(localStorage.getItem('contractAddress') || '');
	let contractAbi = $state<any>(undefined);
	let iframeRef = $state<HTMLIFrameElement>();
	let generatedUI = $state<string>('');
	let openAiKey = $state<string>(localStorage.getItem('openAiKey') || '');

	// Add a mounted flag
	let iframeMounted = $state(false);

	// Create a handler for when the iframe is mounted
	const handleIframeMount = (node: HTMLIFrameElement) => {
			console.log('Iframe mounted');
			iframeRef = node;
			iframeMounted = true;
	};

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

	const handleAbi = async () => {
		try {
			const trimmedAddress = contractAddress.trim();
			const result = await whatsabi.autoload(trimmedAddress, { 
				provider: client 
			});
			contractAbi = result.abi;

			// Try to get contract name if available
			let contractName = 'Unknown Contract';
			try {
				const nameFunction = result.abi.find(item => 
					item.type === 'function' && 
					item.name === 'name' && 
					item.inputs?.length === 0
				);
				
				if (nameFunction) {
					const name = await client.readContract({
						address: trimmedAddress,
						abi: [nameFunction],
						functionName: 'name',
					});
					contractName = name as string;
				}
			} catch (error) {
				console.warn('Could not fetch contract name:', error);
			}

			const response = await fetch('https://api.openai.com/v1/chat/completions', {
				method: 'POST',
					headers: {
						'Content-Type': 'application/json',
						'Authorization': `Bearer ${openAiKey}`
					},
					body: JSON.stringify({
						model: "gpt-4-turbo-preview",
						messages: [{
							role: "system",
							content: "You are an expert frontend developer specializing in creating intuitive, well-structured HTML interfaces. Focus on clean, minimal styling while maintaining excellent usability. Group similar functionality together logically. Include proper error handling and loading states. Output only pure HTML code without any markdown formatting, code block tags, or explanations. The output will be directly injected into an HTML document."
						}, {
							role: "user",
							content: `Create a user interface for interacting with the "${contractName ?? trimmedAddress}" smart contract. Requirements:
							- Output raw HTML without any markdown tags or code blocks (no \`\`\`html or similar)
							- Use only HTML and inline CSS (no external dependencies)
							- Group similar functions together (read operations, write operations, events)
							- Include minimal but professional styling
							- Add proper error handling and success messages
							- Make all inputs properly labeled and intuitive
							- Add loading states for async operations
							- Use the globally available 'viemProvider' object for blockchain interactions
							  (it's a Viem-compatible provider injected as window.viemProvider)
							- Contract address is: ${trimmedAddress}
							- Think like a designer. Don't just put a series of inputs try to make an intuitive pleasant UI
							- Make use of modals when it makes sense
							- When building this UI first think about what the contract looks like and think of a product doc first
							- after making a product doc then create an initial versioon of the UI
							- after making the initial version review your UI to see if it's possible to polish and make better
							- finally do a final round of style and css polish
							
							Here's the ABI: ${JSON.stringify(result.abi)}`
						}],
						temperature: 0.7
					})
			});

			const data = await response.json();
			const generatedUI = data.choices?.[0]?.message?.content;
			if (!generatedUI) return;

			// Get iframe reference and inject content
			const iframe = document.querySelector('iframe');
			if (!iframe) return;

			const doc = iframe.contentWindow?.document;
			if (!doc) return;

			// Create and inject the provider before writing content
			const provider = {
				request: async (request: any) => {
					return invoke('request', {
						request: {
							...request,
							jsonrpc: '2.0',
							id: crypto.randomUUID(),
						}
					}).then((response: any) => {
						if (response.error) throw response.error;
						return response.result;
					});
				}
			};

			doc.open();
			doc.write(generatedUI);
			(iframe.contentWindow as any).viemProvider = provider;
			doc.close();

			console.log('UI and provider injected successfully');

		} catch (error) {
			console.error('Error:', error);
		}
	};

	$effect(() => {
		if (!startMessage) return;

		let timeoutId: NodeJS.Timeout;
		let isRunning = true
		const pollBlock = async () => {
				await getBlock();
				await handleAbi();
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

	$effect(() => {
		localStorage.setItem('contractAddress', contractAddress);
	});

	$effect(() => {
		localStorage.setItem('openAiKey', openAiKey);
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

<div>
  <h2>Contract ABI Explorer</h2>
  <input 
    type="text" 
    bind:value={contractAddress} 
    placeholder="Enter contract address"
  />
  <button onclick={handleAbi}>Get ABI</button>

  <input 
    type="password"
    bind:value={openAiKey}
    placeholder="Enter OpenAI API Key"
  />

  <iframe 
    sandbox="allow-scripts allow-same-origin"
    style="width: 100%; height: 800px; border: 2px solid red; margin-top: 20px;"
	title="Generated UI"
  ></iframe>
</div>

<p>Tauri <a href="https://v1.tauri.app/v1/guides/getting-started/setup/sveltekit">docs</a></p>
<p>Sveltekit <a href="https://svelte.dev/docs/kit">docs</a></p>
