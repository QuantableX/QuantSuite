export type CalcMode = "calculator" | "converter";

export interface CalcHistoryEntry {
  id: string;
  expression: string;
  result: string;
  timestamp: number;
}

const UNIT_CATEGORIES: Record<
  string,
  {
    units: string[];
    convert: (val: number, from: string, to: string) => number;
  }
> = {
  Length: {
    units: ["m", "km", "cm", "mm", "in", "ft", "yd", "mi"],
    convert(val, from, to) {
      const toM: Record<string, number> = {
        m: 1,
        km: 1000,
        cm: 0.01,
        mm: 0.001,
        in: 0.0254,
        ft: 0.3048,
        yd: 0.9144,
        mi: 1609.344,
      };
      return (val * toM[from]) / toM[to];
    },
  },
  Weight: {
    units: ["kg", "g", "mg", "lb", "oz", "t"],
    convert(val, from, to) {
      const toKg: Record<string, number> = {
        kg: 1,
        g: 0.001,
        mg: 0.000001,
        lb: 0.453592,
        oz: 0.0283495,
        t: 1000,
      };
      return (val * toKg[from]) / toKg[to];
    },
  },
  Temperature: {
    units: ["°C", "°F", "K"],
    convert(val, from, to) {
      let c =
        from === "°C"
          ? val
          : from === "°F"
            ? ((val - 32) * 5) / 9
            : val - 273.15;
      return to === "°C" ? c : to === "°F" ? (c * 9) / 5 + 32 : c + 273.15;
    },
  },
  Volume: {
    units: ["L", "mL", "gal", "qt", "fl oz", "m³"],
    convert(val, from, to) {
      const toL: Record<string, number> = {
        L: 1,
        mL: 0.001,
        gal: 3.78541,
        qt: 0.946353,
        "fl oz": 0.0295735,
        "m³": 1000,
      };
      return (val * toL[from]) / toL[to];
    },
  },
  Speed: {
    units: ["m/s", "km/h", "mph", "knots"],
    convert(val, from, to) {
      const toMs: Record<string, number> = {
        "m/s": 1,
        "km/h": 1 / 3.6,
        mph: 0.44704,
        knots: 0.514444,
      };
      return (val * toMs[from]) / toMs[to];
    },
  },
  Time: {
    units: ["s", "min", "hr", "day", "week"],
    convert(val, from, to) {
      const toS: Record<string, number> = {
        s: 1,
        min: 60,
        hr: 3600,
        day: 86400,
        week: 604800,
      };
      return (val * toS[from]) / toS[to];
    },
  },
};

export function useGeneralCalc() {
  const mode = ref<CalcMode>("calculator");
  const display = ref("0");
  const expression = ref("");
  const hasResult = ref(false);
  const history = ref<CalcHistoryEntry[]>([]);

  // Converter state
  const category = ref("Length");
  const fromUnit = ref("m");
  const toUnit = ref("km");
  const fromValue = ref("");
  const toValue = ref("");

  function pressKey(key: string) {
    if (hasResult.value && /[0-9.]/.test(key)) {
      display.value = key;
      expression.value = key;
      hasResult.value = false;
      return;
    }
    hasResult.value = false;

    if (key === "C") {
      display.value = "0";
      expression.value = "";
      return;
    }
    if (key === "⌫") {
      if (display.value.length <= 1) display.value = "0";
      else display.value = display.value.slice(0, -1);
      expression.value = expression.value.slice(0, -1);
      return;
    }
    if (key === "±") {
      if (display.value.startsWith("-")) {
        display.value = display.value.slice(1);
        expression.value = expression.value.replace(/^-/, "");
      } else if (display.value !== "0") {
        display.value = "-" + display.value;
        expression.value = "-" + expression.value;
      }
      return;
    }
    if (key === "=") {
      try {
        const rawExpr = expression.value;
        // Convert display symbols to math operators before evaluation
        const mathExpr = rawExpr.replace(/÷/g, "/").replace(/×/g, "*");
        const sanitized = mathExpr.replace(/[^0-9+\-*/.()% ]/g, "");
        const processed = sanitized.replace(/(\d+(\.\d+)?)%/g, "($1/100)");
        const result = Function('"use strict"; return (' + processed + ")")();
        const formatted = Number.isFinite(result)
          ? parseFloat(result.toFixed(10)).toString()
          : "Error";
        display.value = formatted;
        expression.value = formatted;
        hasResult.value = true;
        // Add to history
        if (formatted !== "Error") {
          history.value.unshift({
            id:
              Date.now().toString(36) + Math.random().toString(36).slice(2, 6),
            expression: rawExpr,
            result: formatted,
            timestamp: Date.now(),
          });
          if (history.value.length > 20) history.value.length = 20;
        }
      } catch {
        display.value = "Error";
        expression.value = "";
      }
      return;
    }
    if (display.value === "0" && /[0-9]/.test(key)) display.value = key;
    else display.value += key;
    expression.value += key;
  }

  function convertUnits() {
    const cat = UNIT_CATEGORIES[category.value];
    if (!cat || !fromValue.value) {
      toValue.value = "";
      return;
    }
    const val = parseFloat(fromValue.value);
    if (isNaN(val)) {
      toValue.value = "";
      return;
    }
    const result = cat.convert(val, fromUnit.value, toUnit.value);
    toValue.value = parseFloat(result.toFixed(10)).toString();
  }

  function swapUnits() {
    const tmp = fromUnit.value;
    fromUnit.value = toUnit.value;
    toUnit.value = tmp;
    const tmpVal = fromValue.value;
    fromValue.value = toValue.value;
    toValue.value = tmpVal;
  }

  function setCategory(cat: string) {
    category.value = cat;
    const units = UNIT_CATEGORIES[cat]?.units || [];
    fromUnit.value = units[0] || "";
    toUnit.value = units[1] || units[0] || "";
    fromValue.value = "";
    toValue.value = "";
  }

  function clearHistory() {
    history.value = [];
  }

  function removeHistoryEntry(id: string) {
    history.value = history.value.filter((e) => e.id !== id);
  }

  function replayHistoryEntry(entry: CalcHistoryEntry) {
    expression.value = entry.expression;
    display.value = entry.expression;
    hasResult.value = false;
  }

  const categories = Object.keys(UNIT_CATEGORIES);
  const currentUnits = computed(
    () => UNIT_CATEGORIES[category.value]?.units || [],
  );

  return {
    mode,
    display,
    expression,
    hasResult,
    history,
    category,
    fromUnit,
    toUnit,
    fromValue,
    toValue,
    pressKey,
    convertUnits,
    swapUnits,
    setCategory,
    clearHistory,
    removeHistoryEntry,
    replayHistoryEntry,
    categories,
    currentUnits,
  };
}
