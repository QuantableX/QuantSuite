/**
 * QuantDojo — the curriculum: every track, module and lesson, content included.
 *
 * Data only, and a lazy chunk: `useDojoStore().loadCurriculum()` `import()`s
 * this module when the dojo page mounts or activates, so the terminal chunk
 * (prefetched on idle by the shell's warm cache) does not carry the lesson
 * HTML, and an HMR of the store no longer re-parses it.
 */
import type { Track } from '#terminal/stores/dojo'

export function buildTracks(): Track[] {
  return [
    {
      id: 'finance',
      name: 'Finance Fundamentals',
      icon: 'BookOpen',
      description: 'Market microstructure, technical analysis, risk management',
      color: '#2962ff',
      modules: [
        {
          id: 'finance-market-basics',
          trackId: 'finance',
          name: 'Market Basics',
          description: 'Understanding financial markets and how they operate',
          lessons: [
            {
              id: 'finance-market-basics-1',
              trackId: 'finance',
              moduleId: 'finance-market-basics',
              title: 'What Are Financial Markets?',
              description: 'Learn about different market types and how price discovery works',
              type: 'theory',
              difficulty: 'beginner',
              estimatedMinutes: 8,
              order: 1,
              content: `<h3>Introduction to Financial Markets</h3>
<p>Financial markets are platforms where buyers and sellers come together to trade financial instruments such as stocks, bonds, currencies, and derivatives. They serve a critical role in the global economy by facilitating capital allocation, price discovery, and risk transfer.</p>

<div class="concept-box">
<strong>Key Concept: Price Discovery</strong><br/>
Price discovery is the process by which market participants determine the fair value of an asset through the interaction of supply and demand. When more buyers than sellers exist at a given price, the price rises. When sellers outnumber buyers, the price falls.
</div>

<h4>Types of Financial Markets</h4>
<p><strong>Equity Markets (Stock Markets):</strong> Where shares of publicly traded companies are bought and sold. Examples include the NYSE, NASDAQ, and the London Stock Exchange. Equity markets allow companies to raise capital by selling ownership stakes, while investors can profit from price appreciation and dividends.</p>

<p><strong>Foreign Exchange (Forex):</strong> The largest financial market by daily volume (~$7.5 trillion/day). Currencies are traded in pairs (e.g., EUR/USD). Forex operates 24 hours a day across global time zones, making it highly liquid and accessible.</p>

<p><strong>Cryptocurrency Markets:</strong> Digital asset exchanges that operate 24/7/365 with no market close. Crypto markets are known for higher volatility and lower regulation compared to traditional markets. Major exchanges include Binance, Coinbase, and Kraken.</p>

<p><strong>Commodities Markets:</strong> Where raw materials like gold, oil, wheat, and natural gas are traded. These can be spot markets (immediate delivery) or futures markets (delivery at a future date). Commodities often serve as inflation hedges.</p>

<div class="example-box">
<strong>Example:</strong> When you buy Bitcoin on Binance at $65,000, you are participating in the cryptocurrency market. The price of $65,000 was determined by the collective actions of millions of traders placing buy and sell orders — this is price discovery in action.
</div>

<h4>Market Participants</h4>
<p>Markets are composed of diverse participants: <strong>retail traders</strong> (individuals), <strong>institutional investors</strong> (hedge funds, pension funds), <strong>market makers</strong> (provide liquidity by quoting buy/sell prices), and <strong>algorithmic traders</strong> (use automated systems to execute strategies). Understanding who you are trading against is crucial to developing an edge.</p>`,
            },
            {
              id: 'finance-market-basics-2',
              trackId: 'finance',
              moduleId: 'finance-market-basics',
              title: 'Understanding Order Types',
              description: 'Market orders, limit orders, stops, and when to use each',
              type: 'theory',
              difficulty: 'beginner',
              estimatedMinutes: 10,
              order: 2,
              content: `<h3>Order Types in Trading</h3>
<p>An order is an instruction to buy or sell an asset at a specific price or under specific conditions. Choosing the right order type is fundamental to executing your trading strategy effectively and managing risk.</p>

<div class="concept-box">
<strong>Key Concept: Slippage</strong><br/>
Slippage occurs when the price at which your order executes differs from the price you expected. This is especially common with market orders during volatile conditions or in illiquid markets.
</div>

<h4>Market Orders</h4>
<p>A market order executes immediately at the best available price. It guarantees execution but not the price. Use market orders when you need to enter or exit a position urgently and are willing to accept the current market price. The downside is that in fast-moving or illiquid markets, you may experience significant slippage.</p>

<h4>Limit Orders</h4>
<p>A limit order specifies the maximum price you are willing to pay (buy limit) or the minimum price you are willing to accept (sell limit). Limit orders guarantee price but not execution — your order will only fill if the market reaches your specified price. Traders use limit orders to get better entry prices and to control costs.</p>

<h4>Stop Orders (Stop-Loss)</h4>
<p>A stop order becomes a market order once a specified price (the stop price) is reached. Stop-loss orders are primarily used for risk management — they automatically close a losing position to limit damage. For example, if you buy BTC at $65,000, you might place a stop-loss at $63,000 to limit your loss to roughly 3%.</p>

<h4>Stop-Limit Orders</h4>
<p>A stop-limit combines stop and limit orders. When the stop price is triggered, a limit order is placed instead of a market order. This gives you more price control but risks non-execution if the market moves too fast past your limit price. This is particularly relevant during flash crashes.</p>

<div class="example-box">
<strong>Example:</strong> You want to buy ETH, currently at $3,500. You place a limit buy at $3,400, hoping for a dip. Simultaneously, you set a stop-limit sell at stop=$3,300 / limit=$3,280 as protection. If ETH drops to $3,400, you buy in. If it continues to $3,300, your stop triggers and tries to sell at $3,280 or better.
</div>

<h4>Trailing Stop Orders</h4>
<p>A trailing stop moves with the market price by a fixed percentage or dollar amount. It locks in profits as the price moves favorably while still providing downside protection. For instance, a 5% trailing stop on a long position will always sit 5% below the highest price reached since the order was placed.</p>`,
            },
            {
              id: 'finance-market-basics-3',
              trackId: 'finance',
              moduleId: 'finance-market-basics',
              title: 'Market Basics Quiz',
              description: 'Test your knowledge of financial markets and order types',
              type: 'quiz',
              difficulty: 'beginner',
              estimatedMinutes: 5,
              order: 3,
              content: '<p>Answer the following questions to test your understanding of financial markets and order types.</p>',
              quiz: [
                {
                  id: 'q1',
                  question: 'Which financial market has the highest daily trading volume?',
                  options: ['Stock Market (NYSE)', 'Foreign Exchange (Forex)', 'Cryptocurrency Market', 'Commodities Market'],
                  correctIndex: 1,
                  explanation: 'The Forex market has the highest daily volume at approximately $7.5 trillion per day, dwarfing all other financial markets.',
                },
                {
                  id: 'q2',
                  question: 'What is "price discovery"?',
                  options: [
                    'A trading strategy for finding undervalued assets',
                    'The process of determining fair value through supply and demand',
                    'A tool used by market makers to set prices',
                    'The analysis of historical price charts',
                  ],
                  correctIndex: 1,
                  explanation: 'Price discovery is the process by which market participants collectively determine the fair value of an asset through the interaction of buy and sell orders.',
                },
                {
                  id: 'q3',
                  question: 'Which order type guarantees execution but NOT price?',
                  options: ['Limit Order', 'Stop-Limit Order', 'Market Order', 'Trailing Stop Order'],
                  correctIndex: 2,
                  explanation: 'A market order executes immediately at the best available price, guaranteeing that you get filled but not at what exact price.',
                },
                {
                  id: 'q4',
                  question: 'What is slippage?',
                  options: [
                    'The fee charged by exchanges for each trade',
                    'The time delay between placing and executing an order',
                    'The difference between expected and actual execution price',
                    'A type of trailing stop order',
                  ],
                  correctIndex: 2,
                  explanation: 'Slippage is the difference between the price you expected and the price at which your order actually executed. It commonly occurs with market orders in volatile or illiquid conditions.',
                },
                {
                  id: 'q5',
                  question: 'A trailing stop order is best used for:',
                  options: [
                    'Entering a position at a specific price',
                    'Locking in profits while maintaining upside potential',
                    'Placing a buy order below current market price',
                    'Executing a trade at exactly the current price',
                  ],
                  correctIndex: 1,
                  explanation: 'A trailing stop moves with the market price, locking in profits as the price moves favorably while providing a safety net if the price reverses.',
                },
              ],
            },
            {
              id: 'finance-market-basics-4',
              trackId: 'finance',
              moduleId: 'finance-market-basics',
              title: 'Order Type Selection',
              description: 'Practice choosing the right order type for different scenarios',
              type: 'exercise',
              difficulty: 'beginner',
              estimatedMinutes: 6,
              order: 4,
              content: `<h3>Order Type Selection Exercise</h3>
<p>In this exercise, you will practice selecting the appropriate order type for different trading scenarios. Think about what matters most in each situation: execution speed, price precision, or risk management.</p>

<div class="concept-box">
<strong>Decision Framework</strong><br/>
Ask yourself: Do I need immediate execution? Do I need a specific price? Am I protecting an existing position? Your answers will guide the order type selection.
</div>

<h4>Scenario 1: Urgent Exit</h4>
<p>You hold a long position in SOL. Breaking news just dropped that a major exchange has been hacked, and the price is falling rapidly. You need to exit immediately.</p>
<p><strong>Best choice: Market Order.</strong> In a rapidly declining market, execution speed is paramount. A limit order might not fill if the price drops past your limit, leaving you exposed to further losses.</p>

<h4>Scenario 2: Patient Entry</h4>
<p>You have analyzed BTC and believe $60,000 is a strong support level. Current price is $65,000. You want to buy if BTC dips to that support.</p>
<p><strong>Best choice: Limit Buy Order at $60,000.</strong> There is no urgency, and you want to buy at a specific price. A limit order ensures you only buy at your desired level.</p>

<h4>Scenario 3: Protecting Profits</h4>
<p>You bought ETH at $3,000 and it has rallied to $3,800. You want to protect your gains but also let the position run if it keeps going up.</p>
<p><strong>Best choice: Trailing Stop Order.</strong> Set a trailing stop at, say, 5%. If ETH continues to rise, your stop rises with it. If it reverses 5% from the high, you are automatically stopped out with profits locked in.</p>

<div class="example-box">
<strong>Pro Tip:</strong> Many experienced traders use a combination of order types. For example, entering with a limit order for a good price, then immediately placing a stop-loss and a take-profit order to manage the trade. This is sometimes called a "bracket order" or OCO (one-cancels-other).
</div>`,
            },
          ],
        },
        {
          id: 'finance-ta',
          trackId: 'finance',
          name: 'Technical Analysis',
          description: 'Chart patterns, indicators, and price action analysis',
          lessons: [
            {
              id: 'finance-ta-1',
              trackId: 'finance',
              moduleId: 'finance-ta',
              title: 'Candlestick Charts Explained',
              description: 'OHLC data, candlestick anatomy, and key patterns',
              type: 'theory',
              difficulty: 'beginner',
              estimatedMinutes: 10,
              order: 1,
              content: `<h3>Reading Candlestick Charts</h3>
<p>Candlestick charts are the most popular chart type used by traders. Originating from 18th-century Japanese rice trading, each candlestick represents price action over a specific time period and displays four data points: Open, High, Low, and Close (OHLC).</p>

<div class="concept-box">
<strong>Candlestick Anatomy</strong><br/>
<strong>Body:</strong> The thick part between Open and Close. A green/hollow body means Close > Open (bullish). A red/filled body means Close < Open (bearish).<br/>
<strong>Upper Wick (Shadow):</strong> The thin line above the body, showing the highest price reached.<br/>
<strong>Lower Wick (Shadow):</strong> The thin line below the body, showing the lowest price reached.
</div>

<h4>Key Single-Candle Patterns</h4>
<p><strong>Doji:</strong> Open and Close are nearly identical, creating a cross or plus sign shape. A doji signals indecision between buyers and sellers. After a strong trend, a doji can signal a potential reversal. The longer the wicks, the more volatile the indecision.</p>

<p><strong>Hammer / Hanging Man:</strong> A small body at the top with a long lower wick (at least 2x the body length). When appearing after a downtrend it is called a Hammer and is bullish — it shows sellers pushed the price down but buyers fought back aggressively. After an uptrend, the same shape is called a Hanging Man and is bearish.</p>

<p><strong>Engulfing Patterns:</strong> A two-candle pattern where the second candle completely engulfs the body of the first. A Bullish Engulfing (red candle followed by a larger green candle) at support suggests a reversal upward. A Bearish Engulfing at resistance suggests a reversal downward.</p>

<div class="example-box">
<strong>Example:</strong> BTC is in a downtrend, falling from $68,000 to $62,000 over five days. On day six, a hammer candle forms: it opens at $62,100, drops to $60,500 during the day (long lower wick), but closes at $62,000. The next day opens higher and closes at $63,500 — a bullish engulfing. This two-day pattern suggests the downtrend may be ending.
</div>

<h4>Multi-Candle Patterns</h4>
<p><strong>Morning Star / Evening Star:</strong> Three-candle reversal patterns. A Morning Star at the bottom consists of a long red candle, a small-bodied candle (the "star"), and a long green candle. It signals bullish reversal. The Evening Star is the inverse at the top.</p>

<p><strong>Three White Soldiers / Three Black Crows:</strong> Three consecutive long-bodied candles in the same direction, each closing progressively higher (soldiers) or lower (crows). These indicate strong momentum and conviction in the trend direction.</p>`,
            },
            {
              id: 'finance-ta-2',
              trackId: 'finance',
              moduleId: 'finance-ta',
              title: 'Support and Resistance',
              description: 'Identifying key price levels and their significance',
              type: 'theory',
              difficulty: 'beginner',
              estimatedMinutes: 8,
              order: 2,
              content: `<h3>Support and Resistance Levels</h3>
<p>Support and resistance are arguably the most important concepts in technical analysis. They represent price levels where buying or selling pressure historically concentrates, creating "barriers" that price tends to respect.</p>

<div class="concept-box">
<strong>Key Concept: Support</strong><br/>
A support level is a price where buying interest is strong enough to overcome selling pressure, causing the price to bounce upward. Think of it as a "floor" — the more times price bounces off this level, the stronger it becomes.
</div>

<div class="concept-box">
<strong>Key Concept: Resistance</strong><br/>
A resistance level is where selling pressure overcomes buying interest, causing the price to reverse downward. Think of it as a "ceiling" — repeated rejections at a level strengthen it.
</div>

<h4>How to Identify S/R Levels</h4>
<p><strong>Historical Price Action:</strong> Look for areas where price has reversed multiple times. The more touches a level has, the more significant it is. A level that has been tested 5 times is more reliable than one tested only twice.</p>

<p><strong>Round Numbers (Psychological Levels):</strong> Prices like $50,000, $100, or $1.00 often act as support or resistance because traders tend to place orders at round numbers. These are called psychological levels. For BTC, levels like $60,000, $65,000, and $70,000 are closely watched.</p>

<p><strong>Volume Clusters:</strong> Areas where significant trading volume occurred in the past tend to act as S/R. High-volume areas represent prices where many traders have positions, creating anchoring effects.</p>

<h4>Role Reversal</h4>
<p>One of the most powerful concepts in S/R analysis is <strong>role reversal</strong>: when a support level is broken, it often becomes resistance (and vice versa). This happens because traders who bought at the former support level and are now underwater will look to sell at breakeven when price returns to that level.</p>

<div class="example-box">
<strong>Example:</strong> ETH has bounced off $3,000 three times over several weeks (strong support). On the fourth test, it breaks below $3,000 and drops to $2,800. When ETH rallies back to $3,000, it now acts as resistance — sellers who bought at $3,000 use the opportunity to exit at breakeven, creating selling pressure.
</div>`,
            },
            {
              id: 'finance-ta-3',
              trackId: 'finance',
              moduleId: 'finance-ta',
              title: 'Moving Averages',
              description: 'SMA, EMA, crossovers, and trend identification',
              type: 'theory',
              difficulty: 'intermediate',
              estimatedMinutes: 10,
              order: 3,
              content: `<h3>Moving Averages</h3>
<p>Moving averages are among the most widely used technical indicators. They smooth out price data to create a single flowing line, making it easier to identify the direction and strength of a trend. They also serve as dynamic support and resistance levels.</p>

<h4>Simple Moving Average (SMA)</h4>
<p>The SMA is calculated by averaging the closing prices over a specified number of periods. A 20-day SMA adds up the last 20 closing prices and divides by 20. Each day, the oldest price drops off and the newest is added. The SMA gives equal weight to every price in the period.</p>

<h4>Exponential Moving Average (EMA)</h4>
<p>The EMA applies more weight to recent prices, making it more responsive to current market conditions. This is accomplished through a weighting multiplier: 2 / (period + 1). A 20-day EMA reacts faster to price changes than a 20-day SMA, which can be advantageous for short-term trading but may also generate more false signals.</p>

<div class="concept-box">
<strong>Common Periods</strong><br/>
<strong>Short-term:</strong> 9 EMA, 20 SMA — used for scalping and day trading<br/>
<strong>Medium-term:</strong> 50 SMA/EMA — widely watched institutional level<br/>
<strong>Long-term:</strong> 200 SMA — the benchmark for long-term trend direction
</div>

<h4>Moving Average Crossovers</h4>
<p><strong>Golden Cross:</strong> When a shorter-period MA crosses above a longer-period MA, it signals bullish momentum. The most watched golden cross is the 50-day SMA crossing above the 200-day SMA. Historically, this has preceded major bull runs in both stocks and crypto.</p>

<p><strong>Death Cross:</strong> The inverse — the 50-day SMA crossing below the 200-day SMA. This is considered a bearish signal and often triggers institutional selling. However, in crypto markets, death crosses have sometimes been lagging indicators, occurring after most of the downside has already happened.</p>

<div class="example-box">
<strong>Example:</strong> A common EMA crossover strategy uses the 9 EMA and 21 EMA. When the 9 EMA crosses above the 21 EMA, you enter long. When it crosses below, you exit or go short. This is a trend-following system — it works well in trending markets but generates false signals (whipsaws) in sideways markets.
</div>

<h4>MAs as Dynamic Support/Resistance</h4>
<p>In strong uptrends, price often bounces off the 20 or 50 EMA. The 200 SMA acts as a major support/resistance level that institutional traders watch closely. When price is above the 200 SMA, the market is considered to be in a bullish regime; below it, bearish.</p>`,
            },
            {
              id: 'finance-ta-4',
              trackId: 'finance',
              moduleId: 'finance-ta',
              title: 'Technical Analysis Quiz',
              description: 'Test your chart reading and indicator knowledge',
              type: 'quiz',
              difficulty: 'intermediate',
              estimatedMinutes: 5,
              order: 4,
              content: '<p>Test your understanding of candlestick patterns, support/resistance, and moving averages.</p>',
              quiz: [
                {
                  id: 'ta-q1',
                  question: 'A "Doji" candlestick pattern indicates:',
                  options: ['Strong bullish momentum', 'Strong bearish momentum', 'Market indecision', 'A guaranteed trend reversal'],
                  correctIndex: 2,
                  explanation: 'A Doji forms when the open and close are nearly identical, showing that neither buyers nor sellers could gain control — a sign of indecision.',
                },
                {
                  id: 'ta-q2',
                  question: 'When a support level is broken, it often:',
                  options: ['Disappears entirely', 'Becomes a resistance level', 'Becomes a stronger support', 'Has no further significance'],
                  correctIndex: 1,
                  explanation: 'This is called "role reversal." Broken support becomes resistance because trapped buyers at that level look to sell at breakeven when price returns.',
                },
                {
                  id: 'ta-q3',
                  question: 'What is a "Golden Cross"?',
                  options: [
                    'When price crosses above the 200 SMA',
                    'When the 50 SMA crosses above the 200 SMA',
                    'When two candles form a cross pattern',
                    'When volume exceeds the 50-day average',
                  ],
                  correctIndex: 1,
                  explanation: 'A Golden Cross specifically refers to the 50-day SMA crossing above the 200-day SMA, which is considered a major bullish signal.',
                },
                {
                  id: 'ta-q4',
                  question: 'The main advantage of EMA over SMA is:',
                  options: [
                    'It is simpler to calculate',
                    'It gives more weight to recent prices',
                    'It produces fewer false signals',
                    'It works better in sideways markets',
                  ],
                  correctIndex: 1,
                  explanation: 'The EMA applies more weight to recent prices using a weighting multiplier, making it more responsive to current conditions than the SMA which weights all prices equally.',
                },
                {
                  id: 'ta-q5',
                  question: 'A Bullish Engulfing pattern consists of:',
                  options: [
                    'Three consecutive green candles',
                    'A small green candle followed by a larger red candle',
                    'A red candle followed by a larger green candle that engulfs it',
                    'A doji followed by a large candle',
                  ],
                  correctIndex: 2,
                  explanation: 'A Bullish Engulfing is a two-candle pattern where a red (bearish) candle is followed by a larger green (bullish) candle that completely engulfs the body of the red candle, signaling a potential reversal upward.',
                },
              ],
            },
            {
              id: 'finance-ta-5',
              trackId: 'finance',
              moduleId: 'finance-ta',
              title: 'Chart Reading Challenge',
              description: 'Analyze a described chart pattern and decide your trade',
              type: 'challenge',
              difficulty: 'intermediate',
              estimatedMinutes: 8,
              order: 5,
              content: '<p>Read the scenario carefully and decide the best course of action based on what you have learned about technical analysis.</p>',
              challenge: {
                id: 'finance-ta-c1',
                title: 'Chart Reading Challenge',
                description: 'Apply your technical analysis knowledge to a real-world scenario.',
                scenario: `BTC/USDT has been in a downtrend for 2 weeks, falling from $70,000 to $62,000. Today, on the daily chart, you observe the following:

- Price has reached $62,000, which was a strong support level that held three times in the previous month.
- A hammer candle has formed with a lower wick extending down to $60,800 but closing at $62,100.
- The RSI (14) is at 28, indicating oversold conditions.
- The 50-day EMA is at $65,500 (above current price).
- Volume on today's hammer candle is 40% higher than the 20-day average volume.

What is the best trading decision?`,
                options: [
                  {
                    label: 'Enter a long position at $62,100 with stop-loss at $60,500',
                    outcome: 'Excellent decision. The confluence of strong historical support at $62,000, a hammer candle with high volume, and oversold RSI provides a high-probability long setup. The stop-loss below the hammer\'s low ($60,500) gives a clear invalidation point with limited risk. Price rebounds to $66,000 over the next week.',
                    score: 50,
                  },
                  {
                    label: 'Wait for confirmation — enter only if tomorrow closes above $63,000',
                    outcome: 'Good conservative approach. Waiting for confirmation reduces false signals. The next candle closes at $63,400 (bullish engulfing), confirming the reversal. You enter at $63,400 with a stop at $61,800. You capture most of the move to $66,000 but with a slightly worse entry.',
                    score: 35,
                  },
                  {
                    label: 'Short BTC — the downtrend will continue through support',
                    outcome: 'Risky decision. While shorting a downtrend has logic, the confluence of strong support, hammer candle with high volume, and oversold RSI all favor a bounce. Price reverses sharply from $62,000 and your short is stopped out at a loss.',
                    score: 10,
                  },
                  {
                    label: 'Do nothing and wait for the price to break below $60,000',
                    outcome: 'Overly cautious. While patience is a virtue, the setup here has strong confluence. By waiting for $60,000, you miss the reversal entirely as price never reaches that level. It bounces from $62,000 to $66,000 without you.',
                    score: 15,
                  },
                ],
              },
            },
          ],
        },
        {
          id: 'finance-risk',
          trackId: 'finance',
          name: 'Risk Management',
          description: 'Position sizing, stop losses, and capital preservation',
          lessons: [
            {
              id: 'finance-risk-1',
              trackId: 'finance',
              moduleId: 'finance-risk',
              title: 'Position Sizing',
              description: 'How to determine the right trade size to protect your capital',
              type: 'theory',
              difficulty: 'intermediate',
              estimatedMinutes: 10,
              order: 1,
              content: `<h3>Position Sizing: The Foundation of Risk Management</h3>
<p>Position sizing determines how much capital to allocate to each trade. It is arguably the single most important factor in long-term trading success. Even a profitable strategy can blow up an account with improper position sizing, while proper sizing can keep you in the game even during losing streaks.</p>

<div class="concept-box">
<strong>The 1-2% Rule</strong><br/>
Most professional traders risk no more than 1-2% of their total account on any single trade. On a $10,000 account, this means risking $100-$200 per trade. This ensures that even 10 consecutive losses would only draw down the account by 10-20%.
</div>

<h4>Fixed Percentage Method</h4>
<p>The most common approach: risk a fixed percentage of your account on each trade. To calculate position size: <strong>Position Size = (Account * Risk%) / (Entry - StopLoss)</strong>. For example, with a $10,000 account, 2% risk, entry at $100 and stop-loss at $95, you would trade: ($10,000 * 0.02) / ($100 - $95) = 40 shares.</p>

<h4>Kelly Criterion</h4>
<p>The Kelly Criterion is a mathematical formula that calculates the optimal bet size to maximize long-term growth: <strong>f = (bp - q) / b</strong>, where f is the fraction of capital to risk, b is the odds received (reward/risk ratio), p is the win probability, and q is the loss probability (1-p). While mathematically optimal, full Kelly is considered too aggressive — most traders use "half Kelly" or "quarter Kelly" for a smoother equity curve.</p>

<div class="example-box">
<strong>Example: Kelly Calculation</strong><br/>
Your strategy has a 55% win rate and a 2:1 reward-to-risk ratio.<br/>
Kelly % = (2 * 0.55 - 0.45) / 2 = 0.325 = 32.5%<br/>
Half Kelly = 16.25%, Quarter Kelly = 8.125%<br/>
On a $10,000 account, quarter Kelly suggests risking ~$812 per trade.
</div>

<h4>Risk Per Trade vs. Total Exposure</h4>
<p>Beyond individual trade risk, monitor your total portfolio exposure. Having five open positions each risking 2% means 10% total risk. If those positions are correlated (e.g., all crypto longs), your effective risk may be even higher. Limit total open risk to 5-10% of your account.</p>

<p><strong>Volatility-Based Sizing:</strong> Advanced traders adjust position size based on asset volatility. Use the Average True Range (ATR) to normalize position sizes across different assets. Higher volatility = smaller position size. This ensures that each position contributes roughly equal risk regardless of the asset's inherent volatility.</p>`,
            },
            {
              id: 'finance-risk-2',
              trackId: 'finance',
              moduleId: 'finance-risk',
              title: 'Stop Losses & Take Profits',
              description: 'Setting exits, risk-reward ratios, and trailing stops',
              type: 'theory',
              difficulty: 'intermediate',
              estimatedMinutes: 9,
              order: 2,
              content: `<h3>Strategic Exit Planning</h3>
<p>The difference between amateur and professional traders often comes down to exits. Amateurs focus on entries; professionals focus on exits. Knowing when and how to close a trade — both for profit and for loss — is essential to consistent profitability.</p>

<div class="concept-box">
<strong>Risk-to-Reward Ratio (R:R)</strong><br/>
The R:R ratio compares potential loss to potential gain. A trade risking $100 to make $300 has a 1:3 R:R. With a 1:3 R:R, you only need to win 25% of your trades to break even. As a rule of thumb, never take a trade with less than 1:2 R:R.
</div>

<h4>Types of Stop Losses</h4>
<p><strong>Fixed Price Stop:</strong> Set at a specific price level based on technical analysis (below support for longs, above resistance for shorts). This is the most common type and ensures a clear invalidation point for your trade thesis.</p>

<p><strong>Percentage-Based Stop:</strong> A fixed percentage below your entry (e.g., 3%). Simple but does not account for market structure. Use this only when you cannot identify a clear technical level for your stop.</p>

<p><strong>ATR-Based Stop:</strong> Set the stop at a multiple of the Average True Range (e.g., 2x ATR) below entry. This adapts to market volatility — wider stops in volatile markets, tighter in calm markets. A common setting is 1.5-2x the 14-period ATR.</p>

<h4>Take Profit Strategies</h4>
<p><strong>Fixed Target:</strong> Set a specific price target based on the next resistance/support level. Simple and effective but may leave money on the table in strong trends.</p>

<p><strong>Scaled Exits:</strong> Close the position in portions. For example: take 50% profit at 1:2 R:R, move stop to breakeven, then let the remaining 50% run with a trailing stop. This locks in some profit while maintaining exposure to larger moves.</p>

<div class="example-box">
<strong>Example: Complete Trade Plan</strong><br/>
Entry: BTC at $64,000 (breakout above consolidation)<br/>
Stop Loss: $62,500 (below the consolidation range, 1x ATR) = $1,500 risk<br/>
Take Profit 1: $67,000 at 50% position (2:1 R:R, lock in $1,500 on half)<br/>
Take Profit 2: Trail remaining 50% with a 2x ATR trailing stop<br/>
On a $10,000 account with 2% risk: Position size = $200 / $1,500 = 0.133 BTC
</div>

<p><strong>The Breakeven Trap:</strong> Moving your stop to breakeven too quickly can result in getting stopped out before a trade plays out. Give trades enough room to breathe. A good rule: only move to breakeven after the trade has moved 1R in your favor.</p>`,
            },
            {
              id: 'finance-risk-3',
              trackId: 'finance',
              moduleId: 'finance-risk',
              title: 'Risk Management Quiz',
              description: 'Test your understanding of position sizing and risk management',
              type: 'quiz',
              difficulty: 'intermediate',
              estimatedMinutes: 5,
              order: 3,
              content: '<p>Test your knowledge of position sizing, stop losses, and risk management principles.</p>',
              quiz: [
                {
                  id: 'risk-q1',
                  question: 'With a $20,000 account and 2% risk per trade, what is the maximum dollar amount you should risk on a single trade?',
                  options: ['$200', '$400', '$1,000', '$2,000'],
                  correctIndex: 1,
                  explanation: '$20,000 x 2% = $400. This is the maximum amount you should be willing to lose on any single trade.',
                },
                {
                  id: 'risk-q2',
                  question: 'A trade with a 1:3 risk-to-reward ratio means:',
                  options: [
                    'You risk $3 to make $1',
                    'You need a 75% win rate to profit',
                    'You risk $1 to make $3',
                    'You should use 3% position sizing',
                  ],
                  correctIndex: 2,
                  explanation: 'A 1:3 R:R means for every $1 risked, the potential reward is $3. With this ratio, you only need to win more than 25% of your trades to be profitable.',
                },
                {
                  id: 'risk-q3',
                  question: 'What does the Kelly Criterion determine?',
                  options: [
                    'The best time to enter a trade',
                    'The optimal position size for maximum growth',
                    'The correct stop-loss level',
                    'The expected win rate of a strategy',
                  ],
                  correctIndex: 1,
                  explanation: 'The Kelly Criterion calculates the mathematically optimal fraction of capital to risk on each trade to maximize long-term portfolio growth.',
                },
                {
                  id: 'risk-q4',
                  question: 'Why do traders use ATR-based stop losses?',
                  options: [
                    'They are simpler to calculate',
                    'They adapt to market volatility',
                    'They guarantee no losses',
                    'They are required by exchanges',
                  ],
                  correctIndex: 1,
                  explanation: 'ATR (Average True Range) measures market volatility. ATR-based stops widen in volatile markets and tighten in calm markets, adapting to current conditions and reducing the chance of being stopped out by normal price fluctuations.',
                },
                {
                  id: 'risk-q5',
                  question: 'If you have 4 open trades each risking 2% of your account, your total portfolio risk is:',
                  options: ['2%', '4%', '8%', 'It depends on correlation'],
                  correctIndex: 3,
                  explanation: 'While the nominal risk is 8% (4 x 2%), the actual risk depends on correlation between positions. If all four are correlated crypto longs, the effective risk is higher than 8%. Uncorrelated positions provide diversification benefits.',
                },
              ],
            },
            {
              id: 'finance-risk-4',
              trackId: 'finance',
              moduleId: 'finance-risk',
              title: 'Risk Scenario',
              description: 'Calculate position size for a real trading setup',
              type: 'challenge',
              difficulty: 'intermediate',
              estimatedMinutes: 7,
              order: 4,
              content: '<p>Apply position sizing principles to determine the correct trade size for the given scenario.</p>',
              challenge: {
                id: 'finance-risk-c1',
                title: 'Position Sizing Challenge',
                description: 'Calculate the optimal position size for this trading setup.',
                scenario: `You have a $25,000 trading account and follow the 2% risk rule. You have identified a long trade setup on ETH/USDT:

- Current price (entry): $3,500
- Stop-loss level: $3,300 (below key support)
- Take-profit target: $4,100 (next major resistance)
- Risk per share: $3,500 - $3,300 = $200
- Maximum risk: $25,000 x 2% = $500

How should you size this position?`,
                options: [
                  {
                    label: 'Buy 2.5 ETH ($8,750 position) — risk exactly $500 (2%)',
                    outcome: 'Correct calculation. Position size = $500 / $200 = 2.5 ETH. Total position = 2.5 x $3,500 = $8,750 (35% of account). Risk is exactly 2%. R:R ratio is 1:3 ($200 risk, $600 reward). This is a textbook-proper position size with an excellent risk-reward setup.',
                    score: 50,
                  },
                  {
                    label: 'Buy 5 ETH ($17,500 position) — the setup looks strong',
                    outcome: 'Dangerous oversizing. 5 ETH means risking $1,000 (4% of account) and using 70% of capital. Even with a strong setup, doubling your risk rule violates discipline. One bad trade should never threaten more than your predetermined risk level.',
                    score: 10,
                  },
                  {
                    label: 'Buy 1 ETH ($3,500 position) — stay extra conservative',
                    outcome: 'Too conservative for this setup. 1 ETH risks only $200 (0.8% of account). While capital preservation is important, under-sizing a high-quality setup means leaving edge on the table. The risk-reward of 1:3 warrants using the full 2% allocation.',
                    score: 25,
                  },
                  {
                    label: 'Skip the trade — the risk-reward ratio is not good enough',
                    outcome: 'Incorrect assessment. The R:R is $200 risk for $600 reward = 1:3, which is excellent. Most professional traders consider anything above 1:2 to be a good trade. Skipping a well-defined 1:3 setup at strong support means missing a quality opportunity.',
                    score: 5,
                  },
                ],
              },
            },
          ],
        },
      ],
    },
    {
      id: 'quant',
      name: 'Quant Programming',
      icon: 'Code',
      description: 'Python for trading, strategy development, statistical analysis',
      color: '#00bcd4',
      modules: [
        {
          id: 'quant-python',
          trackId: 'quant',
          name: 'Python for Trading',
          description: 'Essential Python skills for quantitative finance',
          lessons: [
            {
              id: 'quant-python-1',
              trackId: 'quant',
              moduleId: 'quant-python',
              title: 'Python Data Structures for Trading',
              description: 'Lists, dictionaries, and DataFrames for market data',
              type: 'theory',
              difficulty: 'beginner',
              estimatedMinutes: 10,
              order: 1,
              content: `<h3>Essential Data Structures</h3>
<p>Trading applications require efficient storage and manipulation of large amounts of market data. Python provides several built-in and library-based data structures that are essential for quantitative trading.</p>

<div class="concept-box">
<strong>OHLCV Data</strong><br/>
The fundamental unit of market data is the OHLCV bar: Open, High, Low, Close, and Volume. Every candle on a chart represents one OHLCV record. Storing and processing this data efficiently is the foundation of all quantitative analysis.
</div>

<h4>Python Lists and Dictionaries</h4>
<p><strong>Lists</strong> are ordered collections ideal for time-series price data. A list of closing prices like <code>[64000, 64500, 63800, 65200, 65800]</code> can be easily sliced, iterated, and used for calculations like moving averages. Lists maintain insertion order, which is critical for time-series data.</p>

<p><strong>Dictionaries</strong> provide key-value storage, perfect for representing a single OHLCV bar: <code>{"open": 64000, "high": 65200, "low": 63500, "close": 64800, "volume": 1250.5}</code>. They offer O(1) lookup time and are used extensively in API responses and configuration.</p>

<h4>Pandas DataFrames</h4>
<p>The <strong>pandas DataFrame</strong> is the workhorse of quantitative finance in Python. It is a 2D labeled data structure — think of it as an Excel spreadsheet in code. For trading, a DataFrame typically has a DatetimeIndex (timestamps) and columns for Open, High, Low, Close, and Volume.</p>

<div class="example-box">
<strong>Example: Creating an OHLCV DataFrame</strong><br/>
<code>import pandas as pd</code><br/>
<code>df = pd.DataFrame({</code><br/>
<code>    'open': [64000, 64500, 63800],</code><br/>
<code>    'high': [65200, 64800, 64500],</code><br/>
<code>    'low': [63500, 63200, 63000],</code><br/>
<code>    'close': [64800, 63800, 64200],</code><br/>
<code>    'volume': [1250, 980, 1100]</code><br/>
<code>}, index=pd.date_range('2024-01-01', periods=3))</code><br/>
<code>df['sma_20'] = df['close'].rolling(20).mean()</code>
</div>

<h4>NumPy Arrays</h4>
<p><strong>NumPy arrays</strong> are the numerical backbone. They offer vectorized operations that are 10-100x faster than Python loops. When you calculate indicators like RSI or Bollinger Bands across thousands of data points, NumPy's vectorized math is essential for performance. Pandas DataFrames are built on top of NumPy arrays.</p>

<p><strong>Key operations:</strong> <code>np.mean()</code>, <code>np.std()</code>, <code>np.log()</code> for log returns, <code>np.cumsum()</code> for cumulative returns, and <code>np.correlate()</code> for correlation analysis. These operations work on entire arrays at once, eliminating the need for slow Python for-loops.</p>`,
            },
            {
              id: 'quant-python-2',
              trackId: 'quant',
              moduleId: 'quant-python',
              title: 'Fetching Market Data',
              description: 'Using APIs and libraries to get real-time and historical data',
              type: 'theory',
              difficulty: 'beginner',
              estimatedMinutes: 9,
              order: 2,
              content: `<h3>Accessing Market Data</h3>
<p>No trading system works without data. Whether you are building a backtester or a live trading bot, you need reliable access to historical and real-time market data. Python offers several excellent libraries and APIs for this purpose.</p>

<h4>CCXT Library</h4>
<p>CCXT (CryptoCurrency eXchange Trading) is a unified library that provides a consistent interface to over 100 cryptocurrency exchanges. Instead of learning each exchange's unique API, you use a single set of methods. This is invaluable for building exchange-agnostic trading systems.</p>

<div class="example-box">
<strong>Example: Fetching OHLCV with CCXT</strong><br/>
<code>import ccxt</code><br/>
<code>exchange = ccxt.binance()</code><br/>
<code>ohlcv = exchange.fetch_ohlcv('BTC/USDT', '1d', limit=100)</code><br/>
<code># Returns: [[timestamp, open, high, low, close, volume], ...]</code><br/>
<code>df = pd.DataFrame(ohlcv, columns=['timestamp', 'open', 'high', 'low', 'close', 'volume'])</code><br/>
<code>df['timestamp'] = pd.to_datetime(df['timestamp'], unit='ms')</code>
</div>

<h4>REST APIs vs. WebSockets</h4>
<p><strong>REST APIs</strong> use HTTP requests to fetch data on demand. They are ideal for historical data, account information, and placing orders. Each request returns a snapshot of data. Rate limits apply — typically 1,200 requests per minute on major exchanges.</p>

<p><strong>WebSockets</strong> maintain a persistent connection and stream data in real-time. They are essential for live trading — order book updates, trade streams, and ticker data arrive as they happen with minimal latency. Use WebSockets for anything that needs to be real-time.</p>

<div class="concept-box">
<strong>Data Quality Matters</strong><br/>
Always validate your data. Common issues include: missing candles (exchange downtime), incorrect timestamps, stale data from API caching, and survivorship bias in historical data. A good data pipeline includes checks for gaps, duplicates, and outliers.
</div>

<h4>Data Cleaning Pipeline</h4>
<p>Raw exchange data often requires cleaning: <strong>1)</strong> Remove duplicate timestamps. <strong>2)</strong> Fill or interpolate missing candles. <strong>3)</strong> Convert timestamps to a consistent timezone. <strong>4)</strong> Verify OHLC relationships (High >= Open, Close; Low <= Open, Close). <strong>5)</strong> Handle volume anomalies (zero-volume candles, extreme spikes). Clean data is the foundation of trustworthy backtests.</p>`,
            },
            {
              id: 'quant-python-3',
              trackId: 'quant',
              moduleId: 'quant-python',
              title: 'Python Trading Quiz',
              description: 'Test your knowledge of Python for quantitative trading',
              type: 'quiz',
              difficulty: 'beginner',
              estimatedMinutes: 5,
              order: 3,
              content: '<p>Test your understanding of Python data structures and data fetching for trading.</p>',
              quiz: [
                {
                  id: 'py-q1',
                  question: 'Which Python library is the standard for handling tabular OHLCV data?',
                  options: ['NumPy', 'Pandas', 'Matplotlib', 'SciPy'],
                  correctIndex: 1,
                  explanation: 'Pandas provides the DataFrame, which is the standard data structure for OHLCV data with its labeled columns, DatetimeIndex, and built-in methods for rolling calculations, resampling, and more.',
                },
                {
                  id: 'py-q2',
                  question: 'What does CCXT provide for crypto traders?',
                  options: [
                    'A charting library for candlestick charts',
                    'A unified API interface to 100+ cryptocurrency exchanges',
                    'Machine learning models for price prediction',
                    'A backtesting framework',
                  ],
                  correctIndex: 1,
                  explanation: 'CCXT is a unified library that normalizes the API interface across 100+ exchanges, letting you write exchange-agnostic code with a single set of methods.',
                },
                {
                  id: 'py-q3',
                  question: 'For real-time price streaming, which connection type should you use?',
                  options: ['REST API', 'WebSocket', 'FTP', 'GraphQL'],
                  correctIndex: 1,
                  explanation: 'WebSockets maintain a persistent connection and push data as it occurs, making them essential for real-time price streams. REST APIs require repeated polling and introduce latency.',
                },
                {
                  id: 'py-q4',
                  question: 'Why are NumPy operations preferred over Python for-loops for indicator calculations?',
                  options: [
                    'NumPy code is easier to read',
                    'NumPy operations are vectorized and 10-100x faster',
                    'Python for-loops have bugs in mathematical operations',
                    'NumPy automatically handles missing data',
                  ],
                  correctIndex: 1,
                  explanation: 'NumPy operations are vectorized, meaning they apply computations to entire arrays at once using optimized C code, which is orders of magnitude faster than iterating through values with Python for-loops.',
                },
                {
                  id: 'py-q5',
                  question: 'Which is NOT a common data cleaning step for OHLCV data?',
                  options: [
                    'Removing duplicate timestamps',
                    'Verifying High >= max(Open, Close)',
                    'Converting all prices to Bitcoin denomination',
                    'Filling or interpolating missing candles',
                  ],
                  correctIndex: 2,
                  explanation: 'Converting to Bitcoin denomination is not a standard cleaning step. Standard steps include deduplication, gap filling, timestamp normalization, and OHLC relationship verification.',
                },
              ],
            },
            {
              id: 'quant-python-4',
              trackId: 'quant',
              moduleId: 'quant-python',
              title: 'Data Pipeline Design',
              description: 'Design a market data fetching and processing pipeline',
              type: 'exercise',
              difficulty: 'beginner',
              estimatedMinutes: 8,
              order: 4,
              content: `<h3>Building a Data Pipeline</h3>
<p>In this exercise, you will learn the components needed to design a robust data pipeline for trading. A good pipeline handles data fetching, cleaning, storage, and serving — each step being critical for reliable strategy execution.</p>

<div class="concept-box">
<strong>Pipeline Architecture</strong><br/>
A typical data pipeline follows this flow:<br/>
<strong>Source</strong> (Exchange API) -> <strong>Fetch</strong> (CCXT/REST) -> <strong>Clean</strong> (Validate/Fill) -> <strong>Store</strong> (Database/Parquet) -> <strong>Serve</strong> (DataFrame/API)
</div>

<h4>Step 1: Data Fetching Layer</h4>
<p>Use CCXT to create an exchange-agnostic fetcher. Implement retry logic with exponential backoff for failed requests. Respect rate limits by tracking request timestamps. For historical data, paginate requests since most exchanges limit responses to 500-1000 candles per request.</p>

<h4>Step 2: Data Validation</h4>
<p>After fetching, validate every record: ensure timestamps are sequential, OHLC values are logically consistent (High is actually the highest value), volume is non-negative, and no duplicate timestamps exist. Log any anomalies for investigation.</p>

<h4>Step 3: Storage Strategy</h4>
<p>For small datasets, CSV or Parquet files work well. For production systems, use a time-series database like TimescaleDB (PostgreSQL extension) or InfluxDB. Parquet is an excellent middle ground — it is columnar, compressed, and extremely fast for analytical queries. Store data with a consistent schema.</p>

<div class="example-box">
<strong>Example Pipeline Structure:</strong><br/>
<code>class DataPipeline:</code><br/>
<code>    def __init__(self, exchange, symbols, timeframe):</code><br/>
<code>        self.exchange = ccxt.binance()</code><br/>
<code>        self.symbols = symbols  # ['BTC/USDT', 'ETH/USDT']</code><br/>
<code>        self.timeframe = timeframe  # '1h'</code><br/>
<code></code><br/>
<code>    def fetch(self, symbol, since=None):</code><br/>
<code>        # Fetch with pagination and rate limiting</code><br/>
<code>    def validate(self, df):</code><br/>
<code>        # Check OHLC logic, gaps, duplicates</code><br/>
<code>    def store(self, df, symbol):</code><br/>
<code>        # Save to parquet with partitioning</code><br/>
<code>    def load(self, symbol, start, end):</code><br/>
<code>        # Load from storage with date filtering</code>
</div>

<h4>Step 4: Incremental Updates</h4>
<p>For ongoing data collection, implement incremental fetching: track the last stored timestamp and only fetch new data. This minimizes API calls and bandwidth. Run updates on a scheduled basis (e.g., every minute for 1m candles, every hour for 1h candles).</p>`,
            },
          ],
        },
        {
          id: 'quant-strategy',
          trackId: 'quant',
          name: 'Strategy Development',
          description: 'Building and testing systematic trading strategies',
          lessons: [
            {
              id: 'quant-strategy-1',
              trackId: 'quant',
              moduleId: 'quant-strategy',
              title: 'Anatomy of a Trading Strategy',
              description: 'Signal generation, entry/exit rules, and parameter design',
              type: 'theory',
              difficulty: 'intermediate',
              estimatedMinutes: 10,
              order: 1,
              content: `<h3>Components of a Systematic Trading Strategy</h3>
<p>A systematic trading strategy replaces subjective decisions with predefined rules. Every strategy, from simple moving average crossovers to complex machine learning models, consists of the same fundamental components.</p>

<div class="concept-box">
<strong>The Five Pillars of a Strategy</strong><br/>
1. <strong>Signal Generation:</strong> What conditions trigger a trade?<br/>
2. <strong>Entry Rules:</strong> How and when do you enter?<br/>
3. <strong>Exit Rules:</strong> How and when do you exit?<br/>
4. <strong>Position Sizing:</strong> How much do you trade?<br/>
5. <strong>Risk Management:</strong> How do you protect capital?
</div>

<h4>Signal Generation</h4>
<p>Signals are the core of your strategy — the logic that identifies trading opportunities. Signals can be based on technical indicators (EMA crossover, RSI extremes), price action (breakouts, candlestick patterns), fundamental data (on-chain metrics, earnings), or statistical models (mean reversion, momentum). A signal should be binary: either conditions are met or they are not.</p>

<h4>Entry and Exit Rules</h4>
<p><strong>Entry rules</strong> define the exact conditions under which you open a position. Be specific: "Buy when 9 EMA crosses above 21 EMA AND RSI is above 50 AND volume is above 20-day average." Vague rules lead to inconsistent execution.</p>

<p><strong>Exit rules</strong> are equally important and often more complex. Define exits for: <strong>profit target reached</strong>, <strong>stop-loss hit</strong>, <strong>signal reversal</strong>, and <strong>time-based exit</strong> (close if trade has not hit target after N bars). Having clear exit rules removes emotion from the equation.</p>

<h4>Parameters and Optimization</h4>
<p>Most strategies have adjustable parameters (e.g., EMA period, RSI threshold). The key challenge is finding parameter values that work well without overfitting to historical data. Use a small number of parameters (fewer than 5), test across multiple market conditions, and prefer round numbers that are likely robust (a 20 EMA is more robust than a 19 EMA).</p>

<div class="example-box">
<strong>Example: Simple EMA Crossover Strategy</strong><br/>
<strong>Signal:</strong> 9 EMA crosses above 21 EMA (bullish), crosses below (bearish)<br/>
<strong>Entry:</strong> Buy at close when bullish signal fires, sell at close when bearish signal fires<br/>
<strong>Exit:</strong> Stop-loss at 2x ATR below entry, take profit at 3x ATR, or signal reversal<br/>
<strong>Sizing:</strong> Risk 1% of equity per trade<br/>
<strong>Parameters:</strong> fast_period=9, slow_period=21, atr_multiplier_stop=2, atr_multiplier_target=3
</div>`,
            },
            {
              id: 'quant-strategy-2',
              trackId: 'quant',
              moduleId: 'quant-strategy',
              title: 'Backtesting Fundamentals',
              description: 'Testing strategies properly and avoiding common pitfalls',
              type: 'theory',
              difficulty: 'intermediate',
              estimatedMinutes: 10,
              order: 2,
              content: `<h3>Backtesting: Testing Your Strategy Against History</h3>
<p>Backtesting simulates how a trading strategy would have performed on historical data. It is the primary tool for evaluating strategies before risking real capital. However, a backtest is only as good as its methodology — there are several critical pitfalls that can make results misleadingly positive.</p>

<div class="concept-box">
<strong>The Golden Rule of Backtesting</strong><br/>
A backtest should simulate reality as closely as possible. Include transaction costs, slippage, and realistic fill assumptions. A strategy that makes 50% in a frictionless backtest might make 10% (or lose money) in the real world.
</div>

<h4>Walk-Forward Testing</h4>
<p>Walk-forward testing divides data into in-sample (training) and out-of-sample (testing) windows. You optimize parameters on the in-sample data, then test on the out-of-sample data. The window then rolls forward and the process repeats. This is the gold standard for strategy validation because out-of-sample data was never seen during optimization.</p>

<h4>Critical Pitfalls</h4>
<p><strong>Look-Ahead Bias:</strong> Using information that would not have been available at the time of the trade. Example: using the daily close price to make a decision at the daily open. Ensure every signal only uses data that was available at the time of execution.</p>

<p><strong>Overfitting:</strong> The most dangerous pitfall. A strategy with many parameters can be perfectly tuned to past data but fail on new data. Signs of overfitting: too many parameters, unusually high backtest returns, poor out-of-sample performance. Combat overfitting by using fewer parameters, testing on multiple assets/timeframes, and requiring statistical significance.</p>

<p><strong>Survivorship Bias:</strong> Testing only on assets that still exist today ignores the many that were delisted or went to zero. In crypto, this is especially relevant — backtesting only on BTC and ETH ignores the thousands of failed tokens.</p>

<div class="example-box">
<strong>Example: Proper Backtest Structure</strong><br/>
<code>Total data: 2020-01-01 to 2024-12-31 (5 years)</code><br/>
<code>Walk-forward windows: 12 months in-sample, 3 months out-of-sample</code><br/>
<code>Window 1: Train 2020 Q1-Q4, Test 2021 Q1</code><br/>
<code>Window 2: Train 2020 Q2 - 2021 Q1, Test 2021 Q2</code><br/>
<code>... and so on, rolling forward each quarter</code><br/>
<code>Final result: concatenated out-of-sample performance across all windows</code>
</div>

<h4>Realistic Assumptions</h4>
<p>Always include: <strong>trading fees</strong> (typically 0.1% maker / 0.1% taker on crypto), <strong>slippage</strong> (add 0.05-0.1% per trade for liquid pairs, more for illiquid), <strong>funding rates</strong> (for perpetual futures, can be +/- 0.01% every 8 hours), and <strong>latency</strong> (the time between signal and execution). These friction costs dramatically impact results, especially for high-frequency strategies.</p>`,
            },
            {
              id: 'quant-strategy-3',
              trackId: 'quant',
              moduleId: 'quant-strategy',
              title: 'Strategy Development Quiz',
              description: 'Test your understanding of strategy building and backtesting',
              type: 'quiz',
              difficulty: 'intermediate',
              estimatedMinutes: 5,
              order: 3,
              content: '<p>Test your knowledge of strategy development and backtesting principles.</p>',
              quiz: [
                {
                  id: 'strat-q1',
                  question: 'What is "look-ahead bias" in backtesting?',
                  options: [
                    'Using too many indicators in a strategy',
                    'Using information that would not have been available at trade time',
                    'Looking at too long a time period',
                    'Not accounting for transaction costs',
                  ],
                  correctIndex: 1,
                  explanation: 'Look-ahead bias occurs when a backtest uses data that was not available at the time of the trading decision, such as using the daily close price to make a decision at the open.',
                },
                {
                  id: 'strat-q2',
                  question: 'What is the primary purpose of walk-forward testing?',
                  options: [
                    'To speed up backtest execution',
                    'To validate strategy on unseen data and detect overfitting',
                    'To find the best parameter values',
                    'To reduce transaction costs',
                  ],
                  correctIndex: 1,
                  explanation: 'Walk-forward testing trains on in-sample data and validates on out-of-sample data, rolling forward over time. This detects overfitting because the test data was never seen during optimization.',
                },
                {
                  id: 'strat-q3',
                  question: 'Which is a sign that a strategy may be overfitted?',
                  options: [
                    'It has 2-3 parameters',
                    'It works on multiple assets and timeframes',
                    'It has extremely high backtest returns but poor out-of-sample results',
                    'It uses a simple moving average crossover',
                  ],
                  correctIndex: 2,
                  explanation: 'A large gap between backtest performance and out-of-sample performance is the classic sign of overfitting — the strategy has learned the noise in historical data rather than true market patterns.',
                },
                {
                  id: 'strat-q4',
                  question: 'A complete trading strategy must include all of the following EXCEPT:',
                  options: [
                    'Entry rules',
                    'Exit rules and stop-losses',
                    'A prediction of future market direction',
                    'Position sizing rules',
                  ],
                  correctIndex: 2,
                  explanation: 'A systematic strategy does not predict the market — it reacts to predefined conditions. It needs entry rules, exit rules, position sizing, and risk management, but NOT a directional prediction.',
                },
                {
                  id: 'strat-q5',
                  question: 'Why should backtests include slippage and transaction costs?',
                  options: [
                    'They are required by exchange regulations',
                    'Because these frictions exist in real trading and significantly impact results',
                    'To make the backtest run faster',
                    'They are only needed for high-frequency strategies',
                  ],
                  correctIndex: 1,
                  explanation: 'Transaction costs and slippage are real frictions that occur on every trade. Ignoring them leads to unrealistically positive backtests. A strategy that is profitable before costs may be unprofitable after including them.',
                },
              ],
            },
            {
              id: 'quant-strategy-4',
              trackId: 'quant',
              moduleId: 'quant-strategy',
              title: 'Strategy Design Challenge',
              description: 'Design entry and exit rules for a given market condition',
              type: 'challenge',
              difficulty: 'intermediate',
              estimatedMinutes: 8,
              order: 4,
              content: '<p>Design a strategy for the given market conditions.</p>',
              challenge: {
                id: 'quant-strategy-c1',
                title: 'Strategy Design Challenge',
                description: 'Choose the best strategy approach for the described market regime.',
                scenario: `You are developing a trading strategy for ETH/USDT on the 4-hour timeframe. After analyzing the last 6 months of data, you observe:

- ETH has been trading in a range between $3,000 and $3,800 for 4 months
- There have been 12 touches of the $3,000 support and 10 touches of $3,800 resistance
- The 200-period SMA is flat at $3,400 (confirming no trend)
- Volume spikes occur near the range boundaries
- Average daily range (ATR) is approximately $120

Which strategy approach should you deploy?`,
                options: [
                  {
                    label: 'Mean reversion: Buy near $3,000 support, sell near $3,800 resistance, with tight stops outside the range',
                    outcome: 'Excellent choice. In a well-defined range-bound market, mean reversion strategies excel. Buying at $3,000-$3,100 with stops at $2,900 and targeting $3,700-$3,800 gives a strong R:R. The 12 support touches and flat SMA confirm the range. You would capture 6-8 profitable round trips over 4 months.',
                    score: 50,
                  },
                  {
                    label: 'Trend following: Use EMA crossover to catch the eventual breakout from the range',
                    outcome: 'Poor choice for current conditions. Trend-following strategies generate many false signals (whipsaws) in range-bound markets. The flat 200 SMA confirms there is no trend. EMA crossovers would trigger repeatedly as price oscillates, resulting in a series of small losses. This strategy type works best after a confirmed breakout.',
                    score: 10,
                  },
                  {
                    label: 'Breakout strategy: Place buy stop above $3,800 and sell stop below $3,000, wait for range to break',
                    outcome: 'Reasonable but premature. The range has held for 4 months, so waiting for a breakout makes sense in theory. However, many range breakouts are false breakouts. A better approach would be to trade the range now AND have breakout orders ready. Also, you miss 4 months of profitable range trading while waiting.',
                    score: 25,
                  },
                  {
                    label: 'Grid trading: Place buy and sell orders at every $100 increment within the range',
                    outcome: 'Decent approach but not optimal. Grid trading works in ranges but lacks the precision of trading at the actual S/R boundaries where probability is highest. Grids also require more capital (orders at every level) and may accumulate positions on one side. A targeted mean reversion approach at the range extremes is more capital-efficient.',
                    score: 30,
                  },
                ],
              },
            },
          ],
        },
        {
          id: 'quant-stats',
          trackId: 'quant',
          name: 'Statistical Analysis',
          description: 'Returns analysis, distributions, and performance metrics',
          lessons: [
            {
              id: 'quant-stats-1',
              trackId: 'quant',
              moduleId: 'quant-stats',
              title: 'Returns Analysis',
              description: 'Log returns, distributions, skewness, and kurtosis',
              type: 'theory',
              difficulty: 'advanced',
              estimatedMinutes: 10,
              order: 1,
              content: `<h3>Understanding Returns</h3>
<p>Returns are the fundamental measure of investment performance. How you calculate and analyze returns has profound implications for strategy evaluation, risk management, and portfolio construction.</p>

<h4>Simple vs. Log Returns</h4>
<p><strong>Simple (arithmetic) returns:</strong> R = (P_t - P_{t-1}) / P_{t-1}. Easy to interpret but not additive over time. A +50% return followed by -50% does not equal 0% — it equals -25%. Simple returns are used for cross-sectional comparisons (comparing multiple assets over the same period).</p>

<p><strong>Logarithmic returns:</strong> r = ln(P_t / P_{t-1}). Log returns ARE additive over time, making them ideal for time-series analysis. The multi-period log return is simply the sum of individual log returns. For small returns, log and simple returns are nearly identical.</p>

<div class="concept-box">
<strong>Why Log Returns Matter</strong><br/>
Log returns have several desirable properties: time-additivity, approximate normality (closer to normal distribution than simple returns), and symmetry in gains/losses. A +5% log return followed by -5% log return returns you to exactly where you started.
</div>

<h4>Return Distributions</h4>
<p>Financial returns are often assumed to follow a normal (Gaussian) distribution, but this is dangerously inaccurate. Real return distributions exhibit <strong>fat tails</strong> — extreme events occur far more frequently than a normal distribution predicts. The 2020 COVID crash and the 2022 crypto winter are examples of "tail events."</p>

<p><strong>Skewness</strong> measures the asymmetry of the distribution. Negative skewness (left-skewed) means large losses are more likely than large gains — this is typical for many financial assets. Positive skewness means the right tail is longer, which is desirable.</p>

<p><strong>Kurtosis</strong> measures how heavy the tails are. A normal distribution has kurtosis of 3 (excess kurtosis = 0). Financial returns typically have excess kurtosis of 3-10, meaning extreme events are 3-10x more likely than a normal distribution would suggest. This is why risk models based on normality consistently underestimate tail risk.</p>

<div class="example-box">
<strong>Example: BTC Daily Returns</strong><br/>
Mean daily return: ~0.15%<br/>
Standard deviation: ~3.5%<br/>
Skewness: -0.3 (slightly negatively skewed)<br/>
Excess kurtosis: ~5.2 (very fat tails)<br/>
This means BTC has frequent small gains, occasional large losses, and extreme moves happen much more often than a normal distribution predicts.
</div>`,
            },
            {
              id: 'quant-stats-2',
              trackId: 'quant',
              moduleId: 'quant-stats',
              title: 'Performance Metrics',
              description: 'Sharpe ratio, Sortino, max drawdown, and Calmar ratio',
              type: 'theory',
              difficulty: 'advanced',
              estimatedMinutes: 10,
              order: 2,
              content: `<h3>Measuring Strategy Performance</h3>
<p>Raw returns alone are insufficient to evaluate a trading strategy. A strategy that returns 50% but had a 60% drawdown is very different from one that returns 30% with a 10% drawdown. Performance metrics normalize returns by risk, allowing meaningful comparison.</p>

<div class="concept-box">
<strong>Key Metrics at a Glance</strong><br/>
<strong>Sharpe Ratio:</strong> Return per unit of total risk (volatility)<br/>
<strong>Sortino Ratio:</strong> Return per unit of downside risk only<br/>
<strong>Max Drawdown:</strong> Largest peak-to-trough decline<br/>
<strong>Calmar Ratio:</strong> Annual return divided by max drawdown
</div>

<h4>Sharpe Ratio</h4>
<p>The Sharpe Ratio = (R_p - R_f) / sigma_p, where R_p is portfolio return, R_f is risk-free rate, and sigma_p is portfolio standard deviation. It measures risk-adjusted return. A Sharpe above 1.0 is considered good, above 2.0 is excellent, and above 3.0 is exceptional (and potentially suspicious). Most legitimate strategies have Sharpe ratios between 0.5 and 2.5.</p>

<p><strong>Limitations:</strong> The Sharpe ratio penalizes upside volatility equally to downside volatility. A strategy with large positive swings gets a lower Sharpe even though those swings are desirable. It also assumes returns are normally distributed.</p>

<h4>Sortino Ratio</h4>
<p>The Sortino Ratio addresses the Sharpe's main weakness by only using downside deviation in the denominator. Sortino = (R_p - R_f) / sigma_d, where sigma_d is the standard deviation of negative returns only. This is a more accurate measure for strategies with asymmetric return profiles (positive skew).</p>

<h4>Maximum Drawdown</h4>
<p>Max Drawdown (MDD) is the largest peak-to-trough decline in portfolio value. It answers the question: "What was the worst-case scenario?" MDD is critical because it determines whether a trader can psychologically and financially survive the strategy's worst period. A common rule: expect your live max drawdown to be 1.5-2x your backtested max drawdown.</p>

<h4>Calmar Ratio</h4>
<p>Calmar Ratio = Annualized Return / Max Drawdown. It directly relates return to the worst-case scenario. A Calmar above 1.0 means your annual return exceeds your max drawdown. Above 2.0 is excellent. The Calmar ratio is favored by many fund managers because it is intuitive and directly measures the return you earn per unit of pain suffered.</p>

<div class="example-box">
<strong>Example: Comparing Two Strategies</strong><br/>
<strong>Strategy A:</strong> 40% return, 15% volatility, 25% max drawdown<br/>
Sharpe = 40/15 = 2.67, Calmar = 40/25 = 1.60<br/><br/>
<strong>Strategy B:</strong> 60% return, 35% volatility, 50% max drawdown<br/>
Sharpe = 60/35 = 1.71, Calmar = 60/50 = 1.20<br/><br/>
Despite Strategy B having higher raw returns, Strategy A is superior on a risk-adjusted basis. Most traders would prefer Strategy A.
</div>`,
            },
            {
              id: 'quant-stats-3',
              trackId: 'quant',
              moduleId: 'quant-stats',
              title: 'Statistics Quiz',
              description: 'Test your understanding of returns and performance metrics',
              type: 'quiz',
              difficulty: 'advanced',
              estimatedMinutes: 5,
              order: 3,
              content: '<p>Test your knowledge of statistical analysis for trading.</p>',
              quiz: [
                {
                  id: 'stats-q1',
                  question: 'Why are log returns preferred over simple returns for time-series analysis?',
                  options: [
                    'They are easier to calculate',
                    'They are time-additive (you can sum them over periods)',
                    'They are always positive',
                    'They account for dividends',
                  ],
                  correctIndex: 1,
                  explanation: 'Log returns are time-additive: the multi-period log return equals the sum of individual period log returns. This property makes them ideal for time-series analysis and statistical modeling.',
                },
                {
                  id: 'stats-q2',
                  question: 'What does "excess kurtosis" measure in a return distribution?',
                  options: [
                    'The average return over time',
                    'The symmetry of the distribution',
                    'The heaviness of the tails relative to a normal distribution',
                    'The correlation between returns',
                  ],
                  correctIndex: 2,
                  explanation: 'Excess kurtosis measures how heavy the tails are compared to a normal distribution. Higher kurtosis means extreme events (crashes, spikes) occur more frequently than the normal distribution predicts.',
                },
                {
                  id: 'stats-q3',
                  question: 'A Sharpe Ratio of 2.0 means:',
                  options: [
                    'The strategy doubled in value',
                    'For every unit of risk, you earn 2 units of excess return',
                    'The max drawdown is 50%',
                    'The strategy wins 200% of the time',
                  ],
                  correctIndex: 1,
                  explanation: 'A Sharpe Ratio of 2.0 means the strategy earns 2 units of excess return (above risk-free rate) for every unit of volatility. This is considered excellent risk-adjusted performance.',
                },
                {
                  id: 'stats-q4',
                  question: 'The Sortino ratio improves on the Sharpe ratio by:',
                  options: [
                    'Using monthly returns instead of daily',
                    'Only considering downside volatility in the denominator',
                    'Including transaction costs',
                    'Using geometric mean instead of arithmetic',
                  ],
                  correctIndex: 1,
                  explanation: 'The Sortino ratio only penalizes downside deviation, not upside volatility. This is more appropriate because traders want to maximize upside volatility while minimizing downside — the Sharpe ratio treats both the same.',
                },
                {
                  id: 'stats-q5',
                  question: 'If a strategy has a Calmar ratio of 0.5, this means:',
                  options: [
                    'The annual return is half of the max drawdown',
                    'The strategy loses money half the time',
                    'The Sharpe ratio is 0.5',
                    'The win rate is 50%',
                  ],
                  correctIndex: 0,
                  explanation: 'Calmar Ratio = Annual Return / Max Drawdown. A Calmar of 0.5 means the annual return is only half the max drawdown — for example, 15% annual return with 30% max drawdown. This is below the ideal threshold of 1.0.',
                },
              ],
            },
            {
              id: 'quant-stats-4',
              trackId: 'quant',
              moduleId: 'quant-stats',
              title: 'Metric Calculation',
              description: 'Calculate performance metrics from sample trading data',
              type: 'exercise',
              difficulty: 'advanced',
              estimatedMinutes: 8,
              order: 4,
              content: `<h3>Calculating Performance Metrics</h3>
<p>In this exercise, you will practice calculating key performance metrics by hand using sample data. This builds intuition for what the numbers mean and how they relate to each other.</p>

<div class="concept-box">
<strong>Sample Data</strong><br/>
A strategy traded for 12 months with monthly returns (%):<br/>
Jan: +8, Feb: +3, Mar: -5, Apr: +12, May: -2, Jun: +6, Jul: -8, Aug: +10, Sep: +4, Oct: -3, Nov: +7, Dec: +5<br/>
Total return: ~41.4% (compounded), Annual volatility: ~21%, Risk-free rate: 5%
</div>

<h4>Step 1: Calculate the Sharpe Ratio</h4>
<p>Average monthly return: (8+3-5+12-2+6-8+10+4-3+7+5)/12 = 3.08%<br/>
Annualized return: 3.08% x 12 = 37%<br/>
Monthly std dev: ~6.07%, Annualized: 6.07% x sqrt(12) = ~21%<br/>
<strong>Sharpe = (37% - 5%) / 21% = 1.52</strong> — a good risk-adjusted return.</p>

<h4>Step 2: Calculate Max Drawdown</h4>
<p>Track cumulative returns and find the largest peak-to-trough:<br/>
After Mar (peak at +11%, drops to +5.5%): ~5% drawdown<br/>
After Jul (peak at ~22%, drops to ~12%): ~8.2% drawdown<br/>
<strong>Max Drawdown is approximately 8.2%</strong> — manageable.</p>

<h4>Step 3: Calculate Calmar Ratio</h4>
<p><strong>Calmar = 37% / 8.2% = 4.51</strong> — this is excellent. The annual return is 4.5x the max drawdown, indicating strong risk-adjusted performance with limited pain.</p>

<div class="example-box">
<strong>Interpretation:</strong> This strategy has a Sharpe of 1.52 (good), max drawdown of ~8.2% (conservative), and Calmar of 4.51 (excellent). The negative months (Mar: -5%, Jul: -8%) are modest compared to positive months. This suggests a robust strategy with controlled risk. In practice, expect live performance to be somewhat worse than backtest — apply a haircut of 20-30%.
</div>

<h4>Step 4: Calculate Sortino Ratio</h4>
<p>Downside returns: -5%, -2%, -8%, -3%. Downside deviation: sqrt(mean(5^2 + 2^2 + 8^2 + 3^2) / 12) = sqrt(8.5) = ~2.92% monthly, annualized = ~10.1%.<br/>
<strong>Sortino = (37% - 5%) / 10.1% = 3.17</strong> — much higher than the Sharpe because upside volatility is excluded.</p>`,
            },
          ],
        },
      ],
    },
    {
      id: 'ml',
      name: 'AI & Machine Learning',
      icon: 'Brain',
      description: 'Machine learning for trading, feature engineering, prediction models',
      color: '#9c27b0',
      modules: [
        {
          id: 'ml-basics',
          trackId: 'ml',
          name: 'ML Basics for Finance',
          description: 'Fundamentals of applying machine learning to trading',
          lessons: [
            {
              id: 'ml-basics-1',
              trackId: 'ml',
              moduleId: 'ml-basics',
              title: 'ML in Trading Overview',
              description: 'Supervised vs unsupervised learning and common trading applications',
              type: 'theory',
              difficulty: 'intermediate',
              estimatedMinutes: 10,
              order: 1,
              content: `<h3>Machine Learning in Trading</h3>
<p>Machine learning offers the potential to discover complex, non-linear patterns in market data that traditional technical analysis cannot capture. However, applying ML to trading is fundamentally different from other ML domains, and naive approaches almost always fail.</p>

<div class="concept-box">
<strong>Why ML in Finance Is Hard</strong><br/>
Financial markets have a low signal-to-noise ratio, non-stationary distributions (the rules change over time), adversarial participants who adapt to discovered patterns, and limited data relative to the complexity of the problem. These challenges make overfitting the default outcome.
</div>

<h4>Supervised Learning</h4>
<p><strong>Classification:</strong> Predicting discrete outcomes like buy/sell/hold signals. You label historical data with outcomes (e.g., "did price go up by 2% in the next 24 hours?") and train a model to predict these labels from features. Common algorithms: Random Forests, XGBoost, Neural Networks.</p>

<p><strong>Regression:</strong> Predicting continuous values like future returns or volatility. Instead of a binary outcome, you predict the actual magnitude. Useful for position sizing (allocate more to higher-conviction predictions) and volatility forecasting.</p>

<h4>Unsupervised Learning</h4>
<p><strong>Clustering:</strong> Grouping similar market regimes (trending, ranging, volatile, calm) without predefined labels. K-means or DBSCAN can identify market states that might require different trading strategies. Regime detection allows you to switch strategies based on current market conditions.</p>

<p><strong>Dimensionality Reduction:</strong> PCA (Principal Component Analysis) can compress hundreds of correlated features into a few principal components, reducing noise while retaining the most important information. This helps combat overfitting by reducing the feature space.</p>

<h4>Common Pitfalls</h4>
<p><strong>Data Leakage:</strong> The ML equivalent of look-ahead bias. Ensure your training data never contains information from the future. This includes not just target variables but also features computed using future data (e.g., a z-score computed over the full dataset instead of using an expanding window).</p>

<p><strong>Non-stationarity:</strong> Market dynamics change over time. A model trained on 2020 data may not work in 2024. Use rolling or expanding windows for training, and monitor model performance for degradation. Retrain regularly.</p>

<div class="example-box">
<strong>Realistic Expectations</strong><br/>
Academic papers showing 85%+ accuracy on stock prediction are typically overfit or have data leakage. In practice, a model with 52-55% directional accuracy that is well-calibrated and properly sized can be highly profitable. Focus on edge consistency, not headline accuracy.
</div>`,
            },
            {
              id: 'ml-basics-2',
              trackId: 'ml',
              moduleId: 'ml-basics',
              title: 'Feature Engineering for Markets',
              description: 'Creating meaningful inputs for ML models from market data',
              type: 'theory',
              difficulty: 'advanced',
              estimatedMinutes: 10,
              order: 2,
              content: `<h3>Feature Engineering: The Key to ML Success</h3>
<p>In ML for trading, feature engineering is often more important than model selection. Features are the inputs your model uses to make predictions. Good features encode meaningful market information; bad features add noise and cause overfitting.</p>

<h4>Technical Indicator Features</h4>
<p>Common technical indicators can serve as features: RSI, MACD, Bollinger Band width, ATR, OBV (On-Balance Volume), and moving average slopes. However, raw indicator values are often less useful than their derivatives: the rate of change of RSI, whether price is above or below the 200 SMA (binary), or the distance from the upper Bollinger Band as a percentage.</p>

<div class="concept-box">
<strong>Feature Engineering Principles</strong><br/>
1. <strong>Normalize:</strong> Use returns, z-scores, or percentile ranks instead of raw prices<br/>
2. <strong>Stationarize:</strong> Ensure features do not trend over time<br/>
3. <strong>Lag properly:</strong> All features must use only past data (no future information)<br/>
4. <strong>Reduce redundancy:</strong> Correlated features add noise without information
</div>

<h4>Lag Features</h4>
<p>Lag features capture momentum and patterns over time. Common lag features include: returns over the last 1, 5, 10, and 20 periods; volume changes over similar windows; high-low range over various lookbacks. The key is choosing the right lookback periods that capture meaningful market dynamics without adding too many features.</p>

<h4>Normalization Methods</h4>
<p><strong>Z-score normalization:</strong> (value - rolling_mean) / rolling_std. This centers features around zero with unit variance, making them comparable across assets and time periods. Use a rolling window (e.g., 252 trading days) to avoid look-ahead bias.</p>

<p><strong>Percentile rank:</strong> Convert values to their percentile within a rolling window. An RSI of 70 might be high normally, but in a strong bull market, 70 could be the 30th percentile. Percentile ranking accounts for changing regimes.</p>

<h4>Alternative Data Features</h4>
<p>Beyond price data: order book imbalance (bid volume vs. ask volume), funding rates (for crypto perpetuals), options-derived metrics (implied volatility, put-call ratio), on-chain data (active addresses, exchange flows), and sentiment indicators (fear/greed index, social media volume).</p>

<div class="example-box">
<strong>Example Feature Set for BTC/USDT</strong><br/>
<code>returns_1h, returns_4h, returns_24h, returns_7d   # Momentum</code><br/>
<code>rsi_14_zscore, macd_histogram_sign                 # Indicators</code><br/>
<code>bb_width_percentile, atr_14_zscore                 # Volatility</code><br/>
<code>volume_ratio_20d, obv_slope_10                     # Volume</code><br/>
<code>funding_rate, open_interest_change                 # Market structure</code><br/>
<code>Total: 12 features — small enough to avoid overfitting</code>
</div>`,
            },
            {
              id: 'ml-basics-3',
              trackId: 'ml',
              moduleId: 'ml-basics',
              title: 'ML Basics Quiz',
              description: 'Test your understanding of machine learning for trading',
              type: 'quiz',
              difficulty: 'intermediate',
              estimatedMinutes: 5,
              order: 3,
              content: '<p>Test your knowledge of applying machine learning to financial markets.</p>',
              quiz: [
                {
                  id: 'ml-q1',
                  question: 'Why is ML in finance harder than in other domains like image recognition?',
                  options: [
                    'Financial data is too large to process',
                    'Low signal-to-noise ratio, non-stationarity, and adversarial participants',
                    'Python is not suited for financial calculations',
                    'Markets are completely random and unpredictable',
                  ],
                  correctIndex: 1,
                  explanation: 'Financial markets have low signal-to-noise ratios, non-stationary distributions (patterns change), and adversarial participants who adapt to discovered edges. This makes overfitting the default outcome.',
                },
                {
                  id: 'ml-q2',
                  question: 'What is "data leakage" in ML for trading?',
                  options: [
                    'When your data is stolen by competitors',
                    'When future information accidentally enters training data',
                    'When your model uses too many features',
                    'When data is lost due to server failure',
                  ],
                  correctIndex: 1,
                  explanation: 'Data leakage occurs when information from the future (or the target variable) accidentally enters the training features, leading to unrealistically high performance that will not replicate in live trading.',
                },
                {
                  id: 'ml-q3',
                  question: 'Why should features be normalized using rolling windows rather than the full dataset?',
                  options: [
                    'Rolling windows are faster to compute',
                    'To avoid look-ahead bias — full dataset normalization uses future data',
                    'Full dataset normalization produces negative values',
                    'Rolling windows produce more features',
                  ],
                  correctIndex: 1,
                  explanation: 'Normalizing with the full dataset uses the mean and standard deviation of ALL data, including future values that would not be available at prediction time. Rolling windows ensure only past data is used.',
                },
                {
                  id: 'ml-q4',
                  question: 'What is the practical accuracy range for a profitable ML trading model?',
                  options: [
                    '90-95% directional accuracy',
                    '75-85% directional accuracy',
                    '52-55% directional accuracy with proper sizing',
                    'ML models cannot be profitable in trading',
                  ],
                  correctIndex: 2,
                  explanation: 'In practice, 52-55% directional accuracy with proper position sizing and risk management can be highly profitable. Claims of 85%+ accuracy almost always indicate overfitting or data leakage.',
                },
                {
                  id: 'ml-q5',
                  question: 'Market regime detection typically uses which type of ML?',
                  options: [
                    'Supervised classification',
                    'Supervised regression',
                    'Unsupervised clustering',
                    'Reinforcement learning',
                  ],
                  correctIndex: 2,
                  explanation: 'Market regime detection uses unsupervised clustering (like K-means or Hidden Markov Models) because market states are not explicitly labeled — the algorithm must discover natural groupings in the data.',
                },
              ],
            },
          ],
        },
        {
          id: 'ml-models',
          trackId: 'ml',
          name: 'Prediction Models',
          description: 'Time series forecasting and classification for trading signals',
          lessons: [
            {
              id: 'ml-models-1',
              trackId: 'ml',
              moduleId: 'ml-models',
              title: 'Time Series Forecasting',
              description: 'ARIMA, LSTM, and transformer models for price prediction',
              type: 'theory',
              difficulty: 'advanced',
              estimatedMinutes: 12,
              order: 1,
              content: `<h3>Time Series Forecasting for Trading</h3>
<p>Time series forecasting attempts to predict future values based on historical patterns. In trading, this typically means predicting future prices, returns, or volatility. Different models suit different aspects of the problem.</p>

<h4>ARIMA Models</h4>
<p>ARIMA (AutoRegressive Integrated Moving Average) is a classical statistical model. It has three components: AR (autoregressive — past values predict future values), I (integrated — differencing to make data stationary), and MA (moving average — past forecast errors predict future values). ARIMA works well for short-term forecasting of relatively stable time series.</p>

<p><strong>Limitations for trading:</strong> ARIMA assumes linear relationships and stationarity. Financial markets are neither. ARIMA forecasts tend to converge to the mean quickly, making them poor for multi-step price prediction. However, ARIMA variants (GARCH) are excellent for volatility forecasting.</p>

<div class="concept-box">
<strong>GARCH for Volatility</strong><br/>
GARCH (Generalized Autoregressive Conditional Heteroskedasticity) models are specifically designed to forecast volatility. Volatility clusters — high volatility periods tend to be followed by high volatility, and vice versa. GARCH captures this behavior and is widely used in options pricing and risk management.
</div>

<h4>LSTM Networks</h4>
<p>Long Short-Term Memory networks are a type of recurrent neural network designed to learn long-range dependencies in sequential data. They maintain a "memory cell" that can selectively remember or forget information over many time steps. For trading, LSTMs can potentially learn complex temporal patterns that linear models cannot.</p>

<p><strong>Practical considerations:</strong> LSTMs require substantial data (years of high-frequency data), careful hyperparameter tuning, and are prone to overfitting on financial data. They work better for volatility prediction than price direction prediction. Use dropout, early stopping, and out-of-sample validation rigorously.</p>

<h4>Transformer Models</h4>
<p>Transformers (the architecture behind GPT) use self-attention mechanisms to process sequences. Unlike LSTMs that process data sequentially, transformers can attend to all parts of the input simultaneously, making them better at capturing long-range dependencies. Recent models like Temporal Fusion Transformers (TFT) have shown promise in financial forecasting.</p>

<div class="example-box">
<strong>Model Selection Guide</strong><br/>
<strong>Volatility forecasting:</strong> GARCH models (proven, interpretable)<br/>
<strong>Short-term price moves:</strong> Gradient boosted trees (XGBoost/LightGBM) with engineered features<br/>
<strong>Complex pattern recognition:</strong> LSTM or Transformer with large datasets<br/>
<strong>General recommendation:</strong> Start simple (linear models, tree ensembles), add complexity only if the simple model underperforms and you have enough data to support it.
</div>`,
            },
            {
              id: 'ml-models-2',
              trackId: 'ml',
              moduleId: 'ml-models',
              title: 'Classification for Trading Signals',
              description: 'Using Random Forests and XGBoost for buy/sell signals',
              type: 'theory',
              difficulty: 'advanced',
              estimatedMinutes: 10,
              order: 2,
              content: `<h3>Classification Models for Trading Signals</h3>
<p>Classification models predict discrete categories — in trading, typically "buy," "sell," or "hold" (or simplified to binary: "up" / "down"). Tree-based ensemble methods are the most practical and widely used approaches in quantitative trading.</p>

<h4>Random Forests</h4>
<p>A Random Forest builds many decision trees, each trained on a random subset of data and features, then takes a majority vote. This "bagging" approach reduces overfitting compared to a single decision tree. Random Forests are robust, require minimal tuning, and provide feature importance rankings that help you understand what drives predictions.</p>

<p><strong>Advantages for trading:</strong> They handle non-linear relationships, missing data, and feature interactions naturally. They do not require feature scaling. The feature importance output helps validate that the model is learning economically meaningful patterns rather than noise.</p>

<h4>XGBoost / LightGBM</h4>
<p>Gradient-boosted tree ensembles (XGBoost, LightGBM, CatBoost) build trees sequentially, where each new tree corrects the errors of the ensemble so far. They typically outperform Random Forests on structured/tabular data and are the go-to choice for most Kaggle competitions and quantitative trading applications.</p>

<div class="concept-box">
<strong>XGBoost Best Practices for Trading</strong><br/>
1. Use purged cross-validation (leave a gap between train and test to prevent leakage)<br/>
2. Set max_depth=3-6 (shallow trees reduce overfitting)<br/>
3. Use early stopping on validation set<br/>
4. Regularize aggressively (high lambda, low learning_rate)<br/>
5. Limit features to 10-20 well-engineered inputs
</div>

<h4>Label Engineering</h4>
<p>How you define the target variable (label) matters enormously. Common approaches:<br/>
<strong>Fixed threshold:</strong> Label "up" if next-period return > 0.5%, "down" if < -0.5%, else "neutral."<br/>
<strong>Triple barrier method:</strong> Label based on which of three barriers (profit target, stop loss, or time horizon) is hit first. This is more realistic as it mirrors actual trading with take-profit and stop-loss levels.</p>

<div class="example-box">
<strong>Example: XGBoost Signal Model</strong><br/>
<code>Features: 15 engineered features (returns, indicators, volume metrics)</code><br/>
<code>Label: Triple barrier method (1% TP, 1% SL, 24h horizon)</code><br/>
<code>Training: Purged walk-forward, 3-month train / 1-month test</code><br/>
<code>Result: 54% accuracy, 1.15 Sharpe on out-of-sample</code><br/>
<code>This modest accuracy translates to real edge because:</code><br/>
<code>- Properly sized positions (Kelly or fixed fractional)</code><br/>
<code>- Consistent across multiple test windows (not overfit)</code><br/>
<code>- Positive expectancy: avg_win * win_rate > avg_loss * loss_rate</code>
</div>

<h4>Model Monitoring</h4>
<p>In production, monitor model performance continuously. Track accuracy, feature drift, and prediction distribution over time. If accuracy drops below a threshold (e.g., below 51%), halt trading and retrain. Markets change — models that worked six months ago may need updated features or retrained weights.</p>`,
            },
            {
              id: 'ml-models-3',
              trackId: 'ml',
              moduleId: 'ml-models',
              title: 'Model Selection Challenge',
              description: 'Choose the right ML approach for a given trading problem',
              type: 'challenge',
              difficulty: 'advanced',
              estimatedMinutes: 7,
              order: 3,
              content: '<p>Apply your machine learning knowledge to select the best approach for this problem.</p>',
              challenge: {
                id: 'ml-models-c1',
                title: 'Model Selection Challenge',
                description: 'Choose the optimal ML approach for the given problem.',
                scenario: `You work at a crypto trading firm and are tasked with building a model to generate daily trading signals for a portfolio of 20 altcoins. You have:

- 3 years of daily OHLCV data for each coin
- 25 engineered features per coin (technical indicators, volume metrics, cross-asset correlations)
- The goal is to predict whether each coin will rise or fall by more than 2% in the next 24 hours
- The model must be interpretable (explain to the risk team why it made each decision)
- Training and prediction must complete within 10 minutes daily

Which modeling approach should you use?`,
                options: [
                  {
                    label: 'XGBoost with purged walk-forward validation and SHAP explanations',
                    outcome: 'Excellent choice. XGBoost handles tabular data with 25 features efficiently, trains in seconds, and provides strong predictive power. Purged walk-forward prevents data leakage. SHAP (SHapley Additive exPlanations) provides per-prediction feature importance, satisfying the interpretability requirement. This is the industry standard approach for this type of problem.',
                    score: 50,
                  },
                  {
                    label: 'Deep LSTM network trained on raw price sequences',
                    outcome: 'Poor fit for this problem. With only 3 years of daily data (~1,095 points per coin), there is not enough data to train a deep LSTM without severe overfitting. LSTMs are also black boxes, failing the interpretability requirement. Training 20 separate LSTM models would be slow and require extensive GPU resources.',
                    score: 10,
                  },
                  {
                    label: 'Logistic regression with L1 regularization',
                    outcome: 'Reasonable but suboptimal. Logistic regression is fast, interpretable, and less prone to overfitting, which are all positives. However, it can only model linear relationships and misses the non-linear interactions between features that tree-based models capture. For 25 features with complex interactions, XGBoost will significantly outperform.',
                    score: 30,
                  },
                  {
                    label: 'Transformer model fine-tuned on all 20 coins simultaneously',
                    outcome: 'Overkill and impractical. Transformers require massive datasets to train effectively — 3 years of daily data is far too little. They are computationally expensive, not interpretable, and the training time may exceed the 10-minute constraint. This approach is better suited for tick-level data with millions of samples.',
                    score: 5,
                  },
                ],
              },
            },
          ],
        },
      ],
    },
    {
      id: 'challenges',
      name: 'Trading Challenges',
      icon: 'Trophy',
      description: 'Real-world trading scenarios to test your decision-making',
      color: '#ff9800',
      modules: [
        {
          id: 'challenges-beginner',
          trackId: 'challenges',
          name: 'Beginner Scenarios',
          description: 'Common trading situations every trader will encounter',
          lessons: [
            {
              id: 'challenges-beginner-1',
              trackId: 'challenges',
              moduleId: 'challenges-beginner',
              title: 'The Breakout',
              description: 'Price consolidating near resistance — decide your trade',
              type: 'challenge',
              difficulty: 'beginner',
              estimatedMinutes: 5,
              order: 1,
              content: '<p>Analyze the breakout setup and make your trading decision.</p>',
              challenge: {
                id: 'ch-beginner-1',
                title: 'The Breakout',
                description: 'A classic breakout scenario requiring entry, stop, and target decisions.',
                scenario: `SOL/USDT has been consolidating in a tight range between $140 and $150 for 10 days on the 4-hour chart. You observe:

- Volume has been decreasing during consolidation (typical before a breakout)
- The 20 EMA is at $145, 50 EMA at $142 (both trending up, bullish)
- BTC is holding strong above $68,000 (positive market environment)
- A large green candle just closed above $150 with 3x average volume
- RSI is at 62 (not overbought yet)

The breakout appears to be happening. What do you do?`,
                options: [
                  {
                    label: 'Enter long at $151, stop-loss at $147 (below range midpoint), target $165',
                    outcome: 'Strong decision. The breakout is confirmed by high volume and a clean close above resistance. Entering at $151 captures the move early. The stop at $147 is well-placed below the range midpoint — if price returns there, the breakout has failed. Target of $165 gives a 1:3.5 R:R. SOL rallies to $168 over the next 5 days.',
                    score: 50,
                  },
                  {
                    label: 'Wait for a pullback to $150 (the broken resistance) before entering',
                    outcome: 'Smart but risky approach. Waiting for a retest of $150 as new support gives a better entry, but strong breakouts often do not pull back. SOL briefly dips to $149.50 intraday but quickly bounces. If you were watching, you caught it; if not, you missed the move to $168 entirely. Good traders place a limit buy at $150.50 and accept missing some breakouts.',
                    score: 35,
                  },
                  {
                    label: 'Short SOL — this looks like a false breakout',
                    outcome: 'Dangerous contrarian play. The evidence overwhelmingly favors the breakout: high volume, clean close above resistance, aligned moving averages, and supportive broader market. Fading a confirmed breakout without counter-evidence leads to a quick loss as SOL surges to $160 within 24 hours.',
                    score: 5,
                  },
                  {
                    label: 'Enter long with a large position — this is a guaranteed move',
                    outcome: 'Right direction, wrong sizing. The breakout setup is strong, but "guaranteed" does not exist in trading. Using a large position violates risk management principles. Even the best setups fail 30-40% of the time. If the breakout reverses, an oversized position can cause significant account damage.',
                    score: 15,
                  },
                ],
              },
            },
            {
              id: 'challenges-beginner-2',
              trackId: 'challenges',
              moduleId: 'challenges-beginner',
              title: 'The Reversal',
              description: 'An overextended move shows signs of exhaustion',
              type: 'challenge',
              difficulty: 'beginner',
              estimatedMinutes: 5,
              order: 2,
              content: '<p>Identify reversal signs and decide how to act.</p>',
              challenge: {
                id: 'ch-beginner-2',
                title: 'The Reversal',
                description: 'Spot the reversal and manage your response.',
                scenario: `ETH/USDT has rallied 25% in 8 days, from $3,200 to $4,000. On the daily chart you see:

- Price is now far above the 20 EMA ($3,600) — extremely extended
- RSI (14) is at 88 — deeply overbought
- The last 3 daily candles show decreasing volume despite rising price (bearish divergence)
- A shooting star candle just formed at $4,000 (long upper wick, small body)
- You are currently long ETH from $3,500 with a $1,000 position, up 14.3%

What should you do with your position?`,
                options: [
                  {
                    label: 'Take partial profit (50-75%) and tighten stop on remainder to breakeven',
                    outcome: 'Excellent risk management. You lock in 7-10% of gains on the majority of the position while keeping some exposure for further upside. The tight stop on the remainder means worst case you keep your partial profits. ETH drops to $3,700 over the next 3 days — your remaining position gets stopped at breakeven, but you kept $75-$107 in profit.',
                    score: 50,
                  },
                  {
                    label: 'Close the entire position and take all profits',
                    outcome: 'Good decision, if slightly aggressive. The warning signs are strong: overbought RSI, volume divergence, and shooting star. Taking the full 14.3% ($143) is solid. ETH does drop to $3,700 before eventually going higher. You exit near the local top. The only downside is missing the eventual move higher, but capital preservation is paramount.',
                    score: 40,
                  },
                  {
                    label: 'Hold and add more — the trend is strong and will continue',
                    outcome: 'Dangerous greed. Multiple warning signs are flashing — adding to an overextended position at overbought levels with declining volume is a recipe for giving back all your gains. ETH drops 7.5% to $3,700 in 3 days, and your increased position turns the 14% win into a breakeven or slight loss.',
                    score: 5,
                  },
                  {
                    label: 'Flip short immediately — this is clearly topping out',
                    outcome: 'Too aggressive. While the reversal signs are strong, flipping directly from long to short requires precision timing. Overbought conditions can persist longer than expected. ETH could spike to $4,200 before reversing, stopping out your short. Better to take profits on the long and wait for confirmation before shorting.',
                    score: 15,
                  },
                ],
              },
            },
            {
              id: 'challenges-beginner-3',
              trackId: 'challenges',
              moduleId: 'challenges-beginner',
              title: 'The News Event',
              description: 'Major news is incoming — manage your existing position',
              type: 'challenge',
              difficulty: 'beginner',
              estimatedMinutes: 5,
              order: 3,
              content: '<p>Navigate a high-impact news event with an open position.</p>',
              challenge: {
                id: 'ch-beginner-3',
                title: 'The News Event',
                description: 'Handle a high-impact news event with an open position.',
                scenario: `You are long BTC/USDT from $65,000 with a position of $5,000 (2x leverage). Current price is $66,500, and you are up 2.3% ($115 unrealized profit). Your stop-loss is at $63,500.

In 30 minutes, the U.S. Federal Reserve will announce its interest rate decision. The market expects a 25bp cut, but there is a 30% probability of no cut (hawkish surprise). You know from experience:

- If the Fed cuts as expected: BTC typically moves +1-3% (priced in, modest reaction)
- If the Fed holds (hawkish surprise): BTC could drop 5-10% rapidly
- Volatility spikes dramatically during the announcement regardless of direction
- Slippage increases significantly during high-impact events
- Your stop-loss might experience 1-2% slippage in a sharp move

How do you manage this situation?`,
                options: [
                  {
                    label: 'Reduce position by 50% before the announcement, tighten stop on the rest',
                    outcome: 'Smart risk management. By halving the position, you lock in ~$57 profit on 50% and reduce your exposure to the volatile event. Tightening the stop on the remaining half limits downside. The risk-reward is asymmetric: the expected +1-3% on a cut versus potential -5-10% on a hold favors reducing exposure. The Fed holds rates — BTC drops 6%. Your reduced position and tighter stop limit the damage.',
                    score: 50,
                  },
                  {
                    label: 'Close the entire position before the announcement',
                    outcome: 'Very conservative but defensible. You bank the $115 profit and avoid all event risk. The Fed holds rates and BTC drops 6% — you avoided a $300+ loss on a leveraged position. The downside: if the Fed had cut, you missed a potential move to $68,000. But avoiding ruin is always more important than missing a single trade.',
                    score: 40,
                  },
                  {
                    label: 'Hold the full position — the stop-loss will protect you',
                    outcome: 'Risky overreliance on stops. In a sharp move, your stop at $63,500 may fill at $62,500 or worse due to slippage. With 2x leverage, a 6% drop from $66,500 means your $65,000 entry drops to ~$62,500 — your actual loss could be $500+ including slippage, more than 10% of your capital. Stop-losses are not guarantees during volatile events.',
                    score: 10,
                  },
                  {
                    label: 'Add to the position — if the Fed cuts, BTC will moon',
                    outcome: 'Extremely risky. Adding leverage before a binary event with 30% chance of a severely negative outcome is gambling, not trading. The expected value is negative because the downside scenario (-5 to -10%) is much larger than the upside (+1 to 3%). The Fed holds, BTC drops 6%, and your increased leveraged position results in a devastating loss.',
                    score: 0,
                  },
                ],
              },
            },
            {
              id: 'challenges-beginner-4',
              trackId: 'challenges',
              moduleId: 'challenges-beginner',
              title: 'The Drawdown',
              description: 'Three consecutive losses — manage your psychology',
              type: 'challenge',
              difficulty: 'beginner',
              estimatedMinutes: 5,
              order: 4,
              content: '<p>Handle a losing streak with proper trading psychology.</p>',
              challenge: {
                id: 'ch-beginner-4',
                title: 'The Drawdown',
                description: 'Manage your mindset during a losing streak.',
                scenario: `You have been trading a well-backtested EMA crossover strategy that has a historical win rate of 55% and a Sharpe ratio of 1.4. Over the past week, you have taken 3 consecutive losing trades:

- Trade 1: Long BTC, stopped out at -2% ($200 loss)
- Trade 2: Long ETH, stopped out at -1.8% ($180 loss)
- Trade 3: Short SOL, stopped out at -2.2% ($220 loss)

Total losses: -$600 (6% of your $10,000 account). Your strategy just generated a fourth signal: Long BTC at $64,000 with a stop at $62,700.

You are feeling frustrated and questioning your strategy. What do you do?`,
                options: [
                  {
                    label: 'Take the trade with normal position sizing — the strategy edge is intact',
                    outcome: 'Correct decision. A 55% win rate means 45% of trades lose. Three consecutive losses has a 9.1% probability (0.45^3) — not unusual at all. Your strategy is backtested with a 1.4 Sharpe. Deviating from the system because of normal variance is the most common mistake traders make. You take the trade; BTC rallies to $67,000. The $600 drawdown was just statistical noise.',
                    score: 50,
                  },
                  {
                    label: 'Take the trade but with 50% normal position size',
                    outcome: 'Understandable but suboptimal. Reducing size after losses (while increasing after wins) is actually the opposite of what math suggests. If your edge is real, every signal should receive full allocation. By trading smaller, you reduce the recovery when the inevitable winning trades come. BTC rallies 4.7% — at half size, you only recover $150 instead of $300.',
                    score: 30,
                  },
                  {
                    label: 'Skip this trade and review your strategy first',
                    outcome: 'Emotionally driven decision disguised as prudence. Three losses in a row is within normal statistical expectations for a 55% strategy. If you start skipping signals after every short losing streak, you will miss the recovering winners and destroy your strategy\'s edge. Review the strategy after market hours — not during a live signal.',
                    score: 15,
                  },
                  {
                    label: 'Double the position size to recover losses quickly',
                    outcome: 'The revenge trading trap. Doubling down to "get back to even" is the #1 account killer. If this trade also loses (45% chance), you lose $400 instead of $200, bringing total losses to $1,000 (10% of account). Emotional position sizing deviations are how small drawdowns become catastrophic ones.',
                    score: 0,
                  },
                ],
              },
            },
          ],
        },
        {
          id: 'challenges-advanced',
          trackId: 'challenges',
          name: 'Advanced Scenarios',
          description: 'Complex situations requiring quick thinking and crisis management',
          lessons: [
            {
              id: 'challenges-advanced-1',
              trackId: 'challenges',
              moduleId: 'challenges-advanced',
              title: 'The Flash Crash',
              description: 'A sudden 20% drop — manage your portfolio',
              type: 'challenge',
              difficulty: 'advanced',
              estimatedMinutes: 6,
              order: 1,
              content: '<p>Navigate a sudden market crash with active positions.</p>',
              challenge: {
                id: 'ch-advanced-1',
                title: 'The Flash Crash',
                description: 'Manage a portfolio during a sudden market crash.',
                scenario: `It is 3 AM and your portfolio alarm wakes you up. BTC has dropped from $68,000 to $54,400 (-20%) in 45 minutes. Your portfolio:

- Long BTC: $10,000 position from $65,000 (now at $54,400, -16.3%, -$1,630 unrealized)
- Long ETH: $5,000 position from $3,500 (ETH dropped 18% to $2,870, -$900 unrealized)
- Long SOL: $3,000 position from $150 (SOL dropped 25% to $112.50, -$750 unrealized)
- Cash: $7,000

Total portfolio: $25,000 -> $18,720 (-25.1%)

Your stop-losses were set but exchanges experienced extreme slippage during the crash. Some stops did not fill. The crash appears to be triggered by a large exchange experiencing a liquidity crisis (not a fundamental issue with the assets themselves).

Order books are extremely thin. Bid-ask spreads are 10x normal. Social media is in full panic mode.`,
                options: [
                  {
                    label: 'Do NOT panic sell — assess the situation calmly. Close the riskiest position (SOL) and hold BTC/ETH',
                    outcome: 'Excellent crisis management. During flash crashes caused by liquidity events (not fundamental problems), selling everything at extreme lows locks in the worst possible prices. SOL, being the most volatile and smallest position, has the most downside risk. Closing SOL at $112.50 locks in a $750 loss but prevents further damage on the riskiest asset. BTC and ETH recover to $61,000 and $3,200 within 48 hours, recovering much of the unrealized loss.',
                    score: 50,
                  },
                  {
                    label: 'Close all positions immediately to prevent further losses',
                    outcome: 'Understandable but costly. Selling during a flash crash with thin order books means extreme slippage. Your BTC might fill at $53,000 instead of $54,400, ETH at $2,800 instead of $2,870. You lock in ~$3,500 in losses plus slippage. When prices recover 48 hours later, you have no position. You could re-enter, but you have crystallized the bottom.',
                    score: 15,
                  },
                  {
                    label: 'Buy more BTC with the $7,000 cash — this is a buying opportunity',
                    outcome: 'Brave but reckless. While flash crashes can be opportunities, buying aggressively when you are already down 25% and the cause is unclear is irresponsible. If this is NOT just a flash crash and the exchange collapses, prices could go much lower. Never add to losing positions during a crisis when you lack full information. The risk of ruin outweighs the potential gain.',
                    score: 10,
                  },
                  {
                    label: 'Set tight trailing stops on all positions and go back to sleep',
                    outcome: 'Passive approach during an active crisis. With spreads at 10x normal, tight trailing stops will trigger immediately on normal volatility, selling your positions at terrible prices. Going back to sleep during a 20% crash with no risk management is negligent. Active management is required during extreme events — monitor order books, assess the cause, and make informed decisions.',
                    score: 20,
                  },
                ],
              },
            },
            {
              id: 'challenges-advanced-2',
              trackId: 'challenges',
              moduleId: 'challenges-advanced',
              title: 'The Correlation Breakdown',
              description: 'Usually correlated assets are diverging — exploit or avoid?',
              type: 'challenge',
              difficulty: 'advanced',
              estimatedMinutes: 6,
              order: 2,
              content: '<p>Handle a breakdown in asset correlation.</p>',
              challenge: {
                id: 'ch-advanced-2',
                title: 'The Correlation Breakdown',
                description: 'Trade or avoid when correlations break down.',
                scenario: `You run a pairs trading strategy that exploits the historically high correlation (0.92) between BTC and ETH. Your strategy goes long the underperformer and short the outperformer when the spread exceeds 2 standard deviations.

Over the past 5 days, BTC has rallied 12% while ETH has only gained 2%. The ETH/BTC ratio has dropped to its lowest level in 6 months. Your spread signal shows ETH is 3.2 standard deviations below its mean relative to BTC — the strongest signal you have ever seen.

However, you notice:
- Ethereum is facing regulatory scrutiny (new SEC investigation announced)
- Several large ETH holders are moving coins to exchanges (potential selling)
- ETH futures funding rates are deeply negative (-0.05% / 8h)
- BTC dominance is surging (capital rotation from alts to BTC)

Your model says: Long ETH, Short BTC. Your gut says: something fundamental has changed.`,
                options: [
                  {
                    label: 'Override the model — the fundamental context suggests the correlation may not revert',
                    outcome: 'Wise decision. Statistical models assume past patterns continue, but fundamental regime changes can permanently alter correlations. The combination of regulatory scrutiny, whale selling, negative funding, and capital rotation to BTC suggests a structural shift in the BTC/ETH relationship. The spread continues to widen — ETH drops another 8% while BTC gains 5%. Your model\'s largest-ever signal would have been its largest-ever loss.',
                    score: 50,
                  },
                  {
                    label: 'Take the trade with 50% normal size — the statistical edge is strong at 3.2 sigma',
                    outcome: 'A compromise that still loses money. Even at half size, the trade goes against you. The fundamental forces are too strong — the 3.2 sigma reading does not account for a potential regime change. When fundamentals shift, historical standard deviations become meaningless. Half-sizing limits the damage but still results in a loss.',
                    score: 20,
                  },
                  {
                    label: 'Take the full position — 3.2 sigma signals have never failed before',
                    outcome: 'The classic quant blow-up. "It has never happened before" is the most dangerous phrase in trading. The 2008 financial crisis was a "25 sigma event" according to Goldman\'s models — which simply meant the model was wrong, not that the event was impossible. You lose 15% as the spread widens to 5 sigma before you can exit.',
                    score: 5,
                  },
                  {
                    label: 'Take the opposite trade — short ETH, long BTC — ride the new trend',
                    outcome: 'Bold but poorly reasoned. While the direction is right in hindsight, this is not what your strategy does. Trend-following a divergence after it has already moved 3.2 sigma means you are late. You also have no backtested edge for this trade. It works this time, but relying on ad-hoc decisions over systematic approaches leads to inconsistency.',
                    score: 25,
                  },
                ],
              },
            },
            {
              id: 'challenges-advanced-3',
              trackId: 'challenges',
              moduleId: 'challenges-advanced',
              title: 'The Squeeze',
              description: 'A short squeeze is developing — manage your short position',
              type: 'challenge',
              difficulty: 'advanced',
              estimatedMinutes: 6,
              order: 3,
              content: '<p>Navigate a short squeeze with an active short position.</p>',
              challenge: {
                id: 'ch-advanced-3',
                title: 'The Squeeze',
                description: 'Manage a short position during a developing squeeze.',
                scenario: `You shorted DOGE/USDT at $0.15 with a $4,000 position based on fundamental overvaluation analysis. Your stop-loss was at $0.165 (10% above entry). The price drops to $0.135 and you are up $400 (10%).

Suddenly, Elon Musk tweets a DOGE meme. Within an hour:
- DOGE pumps from $0.135 to $0.16 (18.5% move)
- Your position has gone from +$400 to -$267
- Funding rates flip to +0.1%/8h (shorts paying longs)
- Open interest surges 40% (new longs piling in)
- Your stop at $0.165 is about to trigger
- Social media is going viral — retail FOMO is accelerating
- The hourly chart shows a near-vertical move with no pullback

What do you do?`,
                options: [
                  {
                    label: 'Let the stop-loss trigger at $0.165 — accept the loss and move on',
                    outcome: 'The disciplined choice. Your stop was pre-set for a reason — $0.165 was your invalidation level. A Musk tweet changes the short-term dynamics entirely. DOGE continues to pump to $0.22 (+46% from your entry). By honoring your stop, you lose $400 (10% of position, ~4% of account). Those who held their shorts lost 30-46%. Discipline preserved your capital.',
                    score: 50,
                  },
                  {
                    label: 'Close the short immediately at market (before the stop) and go long',
                    outcome: 'Closing the short is right, but flipping long at $0.16 after a vertical move is chasing. DOGE does go to $0.22, so the long would have worked, but entering a volatile meme coin after a tweet-driven pump with no technical setup is pure gambling. You close the short well — that is the important part.',
                    score: 30,
                  },
                  {
                    label: 'Move the stop higher to $0.18 — give the trade more room, the fundamentals are still bearish',
                    outcome: 'Fatal mistake. Moving your stop further away during a squeeze is how small losses become account-destroying ones. DOGE goes to $0.22 — your "adjusted" stop at $0.18 triggers with slippage at $0.185, resulting in a $933 loss (23% of position, ~9% of account). Never widen a stop to avoid being stopped out.',
                    score: 5,
                  },
                  {
                    label: 'Add to the short — the pump is irrational and will reverse',
                    outcome: 'The short squeeze trap. Adding to a losing short during a squeeze is how traders blow up accounts. Meme coin pumps driven by viral social media can continue far beyond any rational analysis. DOGE goes to $0.22, and your doubled short position loses $1,800+. "The market can stay irrational longer than you can stay solvent."',
                    score: 0,
                  },
                ],
              },
            },
            {
              id: 'challenges-advanced-4',
              trackId: 'challenges',
              moduleId: 'challenges-advanced',
              title: 'The Black Swan',
              description: 'An extreme, unprecedented market event — survive it',
              type: 'challenge',
              difficulty: 'advanced',
              estimatedMinutes: 7,
              order: 4,
              content: '<p>Manage your portfolio through an extreme market event.</p>',
              challenge: {
                id: 'ch-advanced-4',
                title: 'The Black Swan',
                description: 'Navigate an unprecedented market crisis.',
                scenario: `Breaking news: A major global stablecoin (used across all major exchanges) has lost its peg and is trading at $0.82, down from $1.00. Panic is spreading across the entire crypto market.

Your portfolio before the event:
- BTC: $15,000 long position (25% of portfolio)
- ETH: $10,000 long position
- Various altcoins: $10,000 across 5 positions
- Stablecoin holdings: $25,000 (in the affected stablecoin)
- Total portfolio: ~$60,000

The affected stablecoin makes up 42% of your portfolio. You are uncertain if it will recover or collapse further. Exchanges are experiencing delays. Some trading pairs using this stablecoin have halted. The event happened 15 minutes ago and the situation is developing rapidly.`,
                options: [
                  {
                    label: 'Convert 80% of the stablecoin to BTC/ETH/other stablecoins immediately, keep 20% as a recovery bet',
                    outcome: 'Strong crisis management. Converting the majority eliminates most of the existential risk. Even at $0.82, losing 18% on $20,000 ($3,600) is better than potentially losing 50-100% if the stablecoin collapses entirely. The 20% retained ($5,000 face value) is a calculated bet on recovery. You execute at ~$0.80 average due to slippage. The stablecoin eventually stabilizes at $0.91 — your retained 20% recovers most losses, and your converted 80% preserved capital.',
                    score: 50,
                  },
                  {
                    label: 'Convert 100% of the stablecoin to other assets immediately',
                    outcome: 'Maximum capital preservation at the cost of recovery potential. You convert $25,000 face value at ~$0.78 average (heavy slippage due to everyone rushing to exit), receiving ~$19,500 in other assets. Total loss: $5,500 (9.2% of portfolio). The stablecoin later recovers to $0.91, meaning you "overpaid" for safety by ~$3,250. But if it had gone to zero, you would have saved $19,500. The insurance was expensive but rational.',
                    score: 35,
                  },
                  {
                    label: 'Hold the stablecoin — the team will restore the peg, this is temporary',
                    outcome: 'Dangerous optimism. While some depegging events are temporary (like USDC in March 2023), others lead to total collapse (UST/Luna in May 2022). Holding $25,000 in a depegged stablecoin based on hope is a gamble. In this scenario, the stablecoin recovers to $0.91 — so holding works out. But the risk was asymmetric: potential recovery of 18% upside vs potential loss of 100%. The expected value of holding is negative.',
                    score: 15,
                  },
                  {
                    label: 'Buy more of the depegged stablecoin — this is a buying opportunity at $0.82',
                    outcome: 'Extremely risky speculation. Buying a depegged stablecoin is like catching a falling knife — you might be right, but the downside is catastrophic. If the stablecoin collapses to zero (like UST), you lose everything you added. In this case it recovers to $0.91, so the trade would have been profitable. But allocating MORE to an asset in crisis when you already have 42% exposure violates every risk management principle.',
                    score: 10,
                  },
                ],
              },
            },
          ],
        },
      ],
    },
  ]
}
