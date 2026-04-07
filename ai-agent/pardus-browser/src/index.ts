#!/usr/bin/env node
/**
 * Pardus Browser Agent CLI
 * 
 * Usage:
 *   pardus-browser-agent                    # Interactive mode
 *   pardus-browser-agent "search for..."     # Single query mode
 *   OPENAI_API_KEY=xxx pardus-browser-agent # With env vars
 * 
 * Environment variables:
 *   OPENAI_API_KEY       - OpenAI API key (required)
 *   OPENAI_BASE_URL      - API base URL (default: https://api.openai.com/v1)
 *   OPENAI_MODEL         - Model to use (default: gpt-4)
 * 
 * Example with OpenRouter:
 *   OPENAI_BASE_URL=https://openrouter.ai/api/v1 \
 *   OPENAI_API_KEY=your_key \
 *   OPENAI_MODEL=anthropic/claude-3-opus \
 *   pardus-browser-agent
 */

import { Agent } from './agent/index.js';
import { browserManager } from './core/index.js';
import { readFileSync, existsSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';

interface Config {
  apiKey: string;
  baseURL?: string;
  model?: string;
}

function loadConfig(): Config {
  // Priority: env vars > config file > error
  const apiKey = process.env.OPENAI_API_KEY;
  
  if (!apiKey) {
    // Try config file
    const configPath = join(homedir(), '.pardus-agent', 'config.json');
    if (existsSync(configPath)) {
      try {
        const config = JSON.parse(readFileSync(configPath, 'utf-8'));
        if (config.apiKey) {
          return {
            apiKey: config.apiKey,
            baseURL: config.baseURL || process.env.OPENAI_BASE_URL,
            model: config.model || process.env.OPENAI_MODEL,
          };
        }
      } catch {
        // Fall through to error
      }
    }
    
    console.error('Error: OPENAI_API_KEY not set');
    console.error('');
    console.error('Set it via environment variable:');
    console.error('  export OPENAI_API_KEY=your_key_here');
    console.error('');
    console.error('Or create ~/.pardus-agent/config.json:');
    console.error('  {');
    console.error('    "apiKey": "your_key_here",');
    console.error('    "baseURL": "https://api.openai.com/v1",');
    console.error('    "model": "gpt-4"');
    console.error('  }');
    console.error('');
    process.exit(1);
  }

  return {
    apiKey,
    baseURL: process.env.OPENAI_BASE_URL,
    model: process.env.OPENAI_MODEL,
  };
}

async function interactiveMode(agent: Agent): Promise<void> {
  console.log('🌐 Pardus Browser Agent');
  console.log('Type your queries below. Type "exit" to quit, "clear" to reset conversation.');
  console.log('');

  const { createInterface } = await import('readline');
  
  const rl = createInterface({
    input: process.stdin,
    output: process.stdout,
    prompt: '> ',
  });

  rl.prompt();

  rl.on('line', async (line) => {
    const input = line.trim();
    
    if (input === 'exit' || input === 'quit') {
      console.log('Cleaning up...');
      await agent.cleanup();
      rl.close();
      return;
    }

    if (input === 'clear') {
      agent.clearHistory();
      console.log('Conversation history cleared.');
      rl.prompt();
      return;
    }

    if (input === '') {
      rl.prompt();
      return;
    }

    try {
      console.log('');
      console.log('🤔 Thinking...');
      console.log('');
      
      const response = await agent.chat(input);
      
      console.log(response);
      console.log('');
    } catch (error) {
      console.error('Error:', error instanceof Error ? error.message : String(error));
    }

    rl.prompt();
  });

  rl.on('close', async () => {
    console.log('\nCleaning up...');
    await agent.cleanup();
    process.exit(0);
  });
}

async function singleQueryMode(agent: Agent, query: string): Promise<void> {
  try {
    console.log('🤔 Processing...');
    console.log('');
    
    const response = await agent.chat(query);
    
    console.log(response);
  } catch (error) {
    console.error('Error:', error instanceof Error ? error.message : String(error));
    process.exit(1);
  } finally {
    await agent.cleanup();
  }
}

async function main(): Promise<void> {
  const config = loadConfig();
  
  const agent = new Agent(browserManager, {
    llmConfig: {
      apiKey: config.apiKey,
      baseURL: config.baseURL,
      model: config.model ?? 'gpt-4',
      temperature: 0.7,
      maxTokens: 4000,
    },
  });

  // Handle Ctrl+C gracefully
  process.on('SIGINT', () => {
    console.log('\n\nReceived SIGINT, cleaning up...');
    agent.cleanup().finally(() => process.exit(0));
  });

  process.on('SIGTERM', () => {
    console.log('\n\nReceived SIGTERM, cleaning up...');
    agent.cleanup().finally(() => process.exit(0));
  });

  const query = process.argv[2];

  if (query) {
    await singleQueryMode(agent, query);
  } else {
    await interactiveMode(agent);
  }
}

main().catch((error) => {
  console.error('Fatal error:', error);
  process.exit(1);
});
