<script setup lang="ts">
definePageMeta({ layout: 'terminal' })

import { Wallet, PieChart, ArrowUpRight, ArrowDownRight } from 'lucide-vue-next'

const holdings = [
  { asset: 'BTC', name: 'Bitcoin', amount: 1.245, value: 83720.52, allocation: 45.2, change: 2.34 },
  { asset: 'ETH', name: 'Ethereum', amount: 15.8, value: 55976.24, allocation: 30.2, change: -1.12 },
  { asset: 'SOL', name: 'Solana', amount: 120, value: 21414.00, allocation: 11.6, change: 5.67 },
  { asset: 'USDT', name: 'Tether', amount: 15421.04, value: 15421.04, allocation: 8.3, change: 0 },
  { asset: 'BNB', name: 'BNB', amount: 14.2, value: 8694.66, allocation: 4.7, change: 0.89 },
]

const totalValue = computed(() => holdings.reduce((sum, h) => sum + h.value, 0))
</script>

<template>
  <div class="terminal-workspace space-y-4 h-full overflow-y-auto">
    <QPageHeading title="Portfolio" meta="Sample data" />

    <div class="grid grid-cols-3 gap-3">
      <div class="card p-4">
        <div class="flex items-center gap-2 mb-2">
          <Wallet class="w-4 h-4" style="color: var(--accent)" />
          <span class="text-[10px] font-medium uppercase tracking-wider" style="color: var(--muted)">Total Value</span>
        </div>
        <div class="text-2xl font-bold font-mono" style="color: var(--text-primary)">
          ${{ totalValue.toLocaleString(undefined, { minimumFractionDigits: 2 }) }}
        </div>
        <span class="text-xs" style="color: var(--positive)">+$2,845.60 (1.56%) today</span>
      </div>
      <div class="card p-4">
        <div class="flex items-center gap-2 mb-2">
          <PieChart class="w-4 h-4" style="color: var(--accent)" />
          <span class="text-[10px] font-medium uppercase tracking-wider" style="color: var(--muted)">Assets</span>
        </div>
        <div class="text-2xl font-bold font-mono" style="color: var(--text-primary)">{{ holdings.length }}</div>
        <span class="text-xs" style="color: var(--text-secondary)">Across 2 exchanges</span>
      </div>
      <div class="card p-4">
        <div class="flex items-center gap-2 mb-2">
          <ArrowUpRight class="w-4 h-4" style="color: var(--positive)" />
          <span class="text-[10px] font-medium uppercase tracking-wider" style="color: var(--muted)">Best Performer</span>
        </div>
        <div class="text-2xl font-bold font-mono" style="color: var(--positive)">SOL</div>
        <span class="text-xs" style="color: var(--positive)">+5.67% today</span>
      </div>
    </div>

    <div class="card">
      <div class="px-4 py-2 border-b" style="border-color: var(--border)">
        <span class="text-xs font-semibold" style="color: var(--text-primary)">Holdings</span>
      </div>
      <table class="w-full text-xs">
        <thead>
          <tr style="color: var(--muted); border-bottom: 1px solid var(--border)">
            <th class="text-left px-4 py-2 font-medium">Asset</th>
            <th class="text-right px-4 py-2 font-medium">Holdings</th>
            <th class="text-right px-4 py-2 font-medium">Value</th>
            <th class="text-right px-4 py-2 font-medium">Allocation</th>
            <th class="text-right px-4 py-2 font-medium">24h Change</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="h in holdings" :key="h.asset" class="border-t hover:brightness-110 cursor-pointer" style="border-color: var(--border)">
            <td class="px-4 py-2.5">
              <div class="flex items-center gap-2">
                <div class="w-6 h-6 rounded-full flex items-center justify-center text-[10px] font-bold text-white" style="background-color: var(--accent)">
                  {{ h.asset.slice(0, 2) }}
                </div>
                <div>
                  <div class="font-semibold" style="color: var(--text-primary)">{{ h.asset }}</div>
                  <div class="text-[10px]" style="color: var(--text-secondary)">{{ h.name }}</div>
                </div>
              </div>
            </td>
            <td class="px-4 py-2.5 text-right font-mono" style="color: var(--text-primary)">{{ h.amount }}</td>
            <td class="px-4 py-2.5 text-right font-mono" style="color: var(--text-primary)">${{ h.value.toLocaleString(undefined, { minimumFractionDigits: 2 }) }}</td>
            <td class="px-4 py-2.5 text-right">
              <div class="flex items-center justify-end gap-2">
                <div class="w-16 h-1.5 rounded-full" style="background-color: var(--surface-3)">
                  <div class="h-full rounded-full" :style="{ width: h.allocation + '%', backgroundColor: 'var(--accent)' }" />
                </div>
                <span class="font-mono" style="color: var(--text-secondary)">{{ h.allocation }}%</span>
              </div>
            </td>
            <td class="px-4 py-2.5 text-right font-mono">
              <span class="flex items-center justify-end gap-1" :style="{ color: h.change >= 0 ? 'var(--positive)' : h.change < 0 ? 'var(--negative)' : 'var(--text-secondary)' }">
                <ArrowUpRight v-if="h.change > 0" class="w-3 h-3" />
                <ArrowDownRight v-if="h.change < 0" class="w-3 h-3" />
                {{ h.change > 0 ? '+' : '' }}{{ h.change.toFixed(2) }}%
              </span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
