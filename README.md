# Lehman_cousins : Automated Statistical Arbitrage Engine

## Overview
Lehman_cousins is a proprietary algorithmic trading infrastructure designed to capture alpha through statistical arbitrage and mean-reversion strategies on fragmented, mid-cap cryptocurrency markets. Engineered in C#, the system deliberately bypasses latency-sensitive high-frequency trading (HFT) to focus instead on structural pricing inefficiencies in low-liquidity digital assets.

## Core Architecture
* **Execution Engine:** Developed in C# / .NET, optimizing asynchronous API calls (REST and WebSockets) for efficient order routing and live order book ingestion.
* **Infrastructure:** Cloud-based deployment (OVH) tailored for continuous uptime, resilient background processing, and manageable latency.
* **Monitoring Dashboard:** Web-based interface providing live exposure analytics, dynamic PnL tracking, and algorithmic health metrics.

## Risk Management & Security
* Implementation of hard-coded maximum drawdown limits and automated kill-switches.
* Secure management of exchange API keys with environment-level isolation to mitigate counterparty and exchange-side risks inherent to unregulated crypto environments.

## Project Scope & Roles
* **Lothaire (Quantitative Research):** Alpha generation, signal processing, quantitative modeling, and historical backtesting.
* **Swann (Financial Engineering & Architecture):** System design, API integrations, secure infrastructure deployment, and full-stack monitoring.
