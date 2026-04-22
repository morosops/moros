import path from 'node:path'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

const projectRoot = path.resolve(__dirname, '..')
const apiProxyTarget = process.env.MOROS_DEV_API_TARGET ?? 'http://178.104.234.112'

export default defineConfig({
  plugins: [react()],
  test: {
    alias: {
      'starkzap': path.join(__dirname, 'src/test/starkzap.ts'),
    },
  },
  build: {
    chunkSizeWarningLimit: 3000,
    rollupOptions: {
      onwarn(warning, warn) {
        const message = typeof warning === 'string' ? warning : warning.message ?? ''
        const sourceId = typeof warning === 'object' && warning && 'id' in warning ? String(warning.id ?? '') : ''

        if (
          message.includes('contains an annotation that Rollup cannot interpret due to the position of the comment') &&
          (
            sourceId.includes('/node_modules/@privy-io/') ||
            sourceId.includes('/node_modules/ox/') ||
            sourceId.includes('/node_modules/@walletconnect/utils/node_modules/ox/')
          )
        ) {
          return
        }

        if (
          message.includes('Use of eval in') &&
          sourceId.includes('/node_modules/@starknet-io/get-starknet-core/')
        ) {
          return
        }

        warn(warning)
      },
      output: {
        manualChunks(id) {
          if (id.includes('/node_modules/@privy-io/')) {
            return 'auth-runtime'
          }
          if (id.includes('/node_modules/react/') || id.includes('/node_modules/react-dom/')) {
            return 'react-vendor'
          }
          if (id.includes('/node_modules/react-router-dom/')) {
            return 'router-vendor'
          }
          if (id.includes('/node_modules/@tanstack/')) {
            return 'query-vendor'
          }
          return undefined
        },
      },
    },
  },
  resolve: {
    alias: [
      {
        find: '@hyperlane-xyz/sdk',
        replacement: path.resolve(__dirname, 'src/vendor/optional/hyperlane-sdk.ts'),
      },
      {
        find: '@hyperlane-xyz/registry',
        replacement: path.resolve(__dirname, 'src/vendor/optional/hyperlane-registry.ts'),
      },
      {
        find: '@hyperlane-xyz/utils',
        replacement: path.resolve(__dirname, 'src/vendor/optional/hyperlane-utils.ts'),
      },
      {
        find: '@solana/web3.js',
        replacement: path.resolve(__dirname, 'src/vendor/optional/solana-web3.ts'),
      },
      {
        find: '@starkzap-internal',
        replacement: path.resolve(__dirname, '../node_modules/starkzap/dist/src'),
      },
    ],
  },
  server: {
    host: '0.0.0.0',
    port: 4174,
    fs: {
      allow: [projectRoot],
    },
    proxy: {
      '/auth': {
        target: apiProxyTarget,
        changeOrigin: true,
      },
      '/coordinator': {
        target: apiProxyTarget,
        changeOrigin: true,
      },
      '/deposit': {
        target: apiProxyTarget,
        changeOrigin: true,
      },
      '/indexer': {
        target: apiProxyTarget,
        changeOrigin: true,
      },
      '/relayer': {
        target: apiProxyTarget,
        changeOrigin: true,
      },
    },
  },
})
