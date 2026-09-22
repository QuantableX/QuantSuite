export interface CalculatorInputs {
  capital: number
  riskPercent: number
  leverage: number
  makerFee: number
  takerFee: number
  entryFeeType: 'maker' | 'taker'
  exitFeeType: 'maker' | 'taker'
}

export interface CalculatorResults {
  posSize1x: number
  posSizeLev: number
  quantity: number
  netProfit: number
  netLoss: number
  rrRatio: number
  breakeven: number
}

export interface LevelInputs {
  entry: number
  tp: number
  sl: number
}

export function useCalculator() {
  const inputs = reactive<CalculatorInputs>({
    capital: 100,
    riskPercent: 1,
    leverage: 5,
    makerFee: 0.02,
    takerFee: 0.06,
    entryFeeType: 'maker',
    exitFeeType: 'taker'
  })

  const levels = reactive<LevelInputs>({
    entry: 0,
    tp: 0,
    sl: 0
  })

  const isLong = ref(true)
  const results = ref<CalculatorResults | null>(null)
  const error = ref<string | null>(null)

  function calculate() {
    error.value = null
    
    const entryPrice = levels.entry
    const tpPrice = levels.tp
    const slPrice = levels.sl
    
    if (entryPrice <= 0) {
      error.value = 'Invalid entry price'
      return null
    }

    const entryFee = inputs.entryFeeType === 'maker' ? inputs.makerFee / 100 : inputs.takerFee / 100
    const exitFee = inputs.exitFeeType === 'maker' ? inputs.makerFee / 100 : inputs.takerFee / 100
    const totalFeeRate = entryFee + exitFee

    const riskDollar = inputs.capital * (inputs.riskPercent / 100)
    
    let riskPerUnit: number
    let profitMove: number
    let breakeven: number

    if (isLong.value) {
      if (slPrice >= entryPrice) {
        error.value = 'SL must be below entry for LONG'
        return null
      }
      riskPerUnit = ((entryPrice - slPrice) / entryPrice) + totalFeeRate
      profitMove = ((tpPrice - entryPrice) / entryPrice) - totalFeeRate
      breakeven = entryPrice * (1 + totalFeeRate)
    } else {
      if (slPrice <= entryPrice) {
        error.value = 'SL must be above entry for SHORT'
        return null
      }
      riskPerUnit = ((slPrice - entryPrice) / entryPrice) + totalFeeRate
      profitMove = ((entryPrice - tpPrice) / entryPrice) - totalFeeRate
      breakeven = entryPrice * (1 - totalFeeRate)
    }

    const posSize1x = riskDollar / riskPerUnit
    const posSizeLev = posSize1x / inputs.leverage
    const quantity = posSize1x / entryPrice
    const netProfit = posSize1x * profitMove
    const netLoss = posSize1x * riskPerUnit
    const rrRatio = netLoss > 0 ? netProfit / netLoss : 0

    results.value = {
      posSize1x,
      posSizeLev,
      quantity,
      netProfit,
      netLoss,
      rrRatio,
      breakeven
    }

    return results.value
  }

  function clear() {
    levels.entry = 0
    levels.tp = 0
    levels.sl = 0
    results.value = null
    error.value = null
  }

  return {
    inputs,
    levels,
    isLong,
    results,
    error,
    calculate,
    clear
  }
}

