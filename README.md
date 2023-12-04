# Fuel Canary Watchtower
A tool to monitor both the Fuel and Ethereum chains and the bridge activities occuring between the two

## Project Layout
<pre>
├── <a href="./src/fuel_watcher.rs">fuel_watcher</a>: handles a thread that watches the Fuel chain
│   ├── <a href="./src/fuel_watcher/fuel_chain.rs">fuel_chain</a>: reads basic data from the Fuel chain
│   ├── <a href="./src/fuel_watcher/fungible_token_contract.rs">fungible_token_contract</a>: handles monitoring events related to the bridge fungible token contracts
├── <a href="./src/ethereum_watcher.rs">ethereum_watcher</a>: handles a thread that watches the Ethereum chain
│   ├── <a href="./src/ethereum_watcher/ethereum_chain.rs">ethereum_chain</a>: reads basic data from the Ethereum chain
│   ├── <a href="./src/ethereum_watcher/state_contract.rs">state_contract</a>: handles interacting with and monitoring events from the Fuel chain state contract
│   ├── <a href="./src/ethereum_watcher/portal_contract.rs">portal_contract</a>: handles interacting with and monitoring events from the Fuel message portal contract
│   ├── <a href="./src/ethereum_watcher/gateway_contract.rs">gateway_contract</a>: handles interacting with and monitoring events from the ERC-20 gateway contract
├── <a href="./src/ethereum_actions.rs">ethereum_actions</a>: handles interactions with the Ethereum chain (pausing contracts)
├── <a href="./src/alerts.rs">alerts</a>: handles logging and pushing out info/alerts
├── <a href="./src/config.rs">config</a>: reads configuration set in the watchtower_config.json file
</pre>

### Config File
An example config file can be found at [watchtower_config.json.example](./watchtower_config.json.example). The following options are available for configuration.
```
fuel_graphql: <fuel chain graphql endpoint>
ethereum_rpc: <ethereum chain rpc endpoint>
ethereum_wallet_key: <optional private key for an ethereum wallet>
state_contract_address: <address of the fuel chain state contract>
portal_contract_address: <address of the fuel message portal contract>
gateway_contract_address: <address of the ERC20 gateway contract>
duplicate_alert_delay: <delay in seconds before pushing the same alert>
fuel_client_watcher: {
  connection_alert: {
    alert_level: <level of alert [None, Info, Warn, Error]>
    alert_action: <(optional) action to take [None, PauseState, PauseGateway, PausePortal, PauseAll]>
  }
  block_production_alert: {
    alert_level: <level of alert [None, Info, Warn, Error]>
    alert_action: <(optional) action to take [None, PauseState, PauseGateway, PausePortal, PauseAll]>
  }
  portal_withdrawal_alerts: [{
      alert_level: <level of alert [None, Info, Warn, Error]>
      alert_action: <(optional) action to take [None, PauseState, PauseGateway, PausePortal, PauseAll]>
      time_frame: <window of time to check for threshold>
      amount: <threshold value which triggers the alert>
    }
    <aditional withdraw alert configs>
  ]
  gateway_withdrawal_alerts: [{
      alert_level: <level of alert [None, Info, Warn, Error]>
      alert_action: <(optional) action to take [None, PauseState, PauseGateway, PausePortal, PauseAll]>
      token_name: <name of token for reporting purposes>
      token_address: <address of the fuel token to monitor>
      time_frame: <window of time to check for threshold>
      amount: <threshold value which triggers the alert>
    }
    <aditional withdraw alert configs>
  ]
}
ethereum_client_watcher: {
  connection_alert: {
    alert_level: <level of alert [None, Info, Warn, Error]>
    alert_action: <(optional) action to take [None, PauseState, PauseGateway, PausePortal, PauseAll]>
  }
  block_production_alert: {
    alert_level: <level of alert [None, Info, Warn, Error]>
    alert_action: <(optional) action to take [None, PauseState, PauseGateway, PausePortal, PauseAll]>
    max_block_time: <max seconds between blocks which triggers alert>
  }
  account_funds_alert: {
    alert_level: <level of alert [None, Info, Warn, Error]>
    alert_action: <(optional) action to take [None, PauseState, PauseGateway, PausePortal, PauseAll]>
    min_balance: <min balance which will trigger alert>
  }
  invalid_state_commit_alert: {
    alert_level: <level of alert [None, Info, Warn, Error]>
    alert_action: <(optional) action to take [None, PauseState, PauseGateway, PausePortal, PauseAll]>
  }
  portal_deposit_alerts: [{
      alert_level: <level of alert [None, Info, Warn, Error]>
      alert_action: <(optional) action to take [None, PauseState, PauseGateway, PausePortal, PauseAll]>
      time_frame: <window of time to check for threshold>
      amount: <threshold value which triggers the alert>
    }
    <aditional deposit alert configs>
  ]
  gateway_deposit_alerts: [{
      alert_level: <level of alert [None, Info, Warn, Error]>
      alert_action: <(optional) action to take [None, PauseState, PauseGateway, PausePortal, PauseAll]>
      token_name: <name of token for reporting purposes>
      token_address: <address of the ethereum token to monitor>
      time_frame: <window of time to check for threshold>
      amount: <threshold value which triggers the alert>
    }
    <aditional deposit alert configs>
  ]
}
```

### Alerts Module
The alerts module is responsible for pushing alerts through to some monitoring service as well as logging data to a log file. Logging is configured in [logging_config.yaml](./logging_config.yaml).

## TODOs
- [ ] Fuel Watcher:
  - [x] implement polling thread
  - [ ] Fuel Chain:
    - [x] verify blocks
    - [x] check chain connection
    - [x] check regular block production
    - [ ] check eth withdrawals
  - [ ] Fungible Token Contract:
    - [ ] check token withdrawals
- [ ] Ethereum Watcher:
  - [x] implement polling thread
  - [x] Ethereum Chain:
    - [x] check chain connection
    - [x] check regular block production
    - [x] check eth withdrawals
  - [ ] Fuel Chain State Contract:
    - [ ] check valid state commits
    - [ ] implement alert on pause action timeout 
    - [ ] implement gas escalator on pause action
  - [ ] Fuel Message Portal Contract:
    - [x] check eth deposits
    - [ ] implement alert on pause action timeout 
    - [ ] implement gas escalator on pause action
  - [ ] ERC20 Gateway deposits:
    - [x] check token withdrawals
    - [ ] implement alert on pause action timeout 
    - [ ] implement gas escalator on pause action
- [x] Config module
- [x] Ethereum Actions module:
  - [x] implement separate thread singleton
  - [x] implement pausing on ethereum contracts
- [ ] Alerts module:
  - [x] implement separate thread singleton
  - [x] set a timer on client startup (gives user some time to quickly fix a bad config before everyone gets alerted of an incorrect error)
  - [ ] buffer message alerts to avoid duplicates
  - [ ] send alerts through a broadcasting service like PagerDuty

### Might Want to Add
- We currently only check that committed blocks match what's in the fuel chain. This does not protect us from a bug in the client that might screw up MessageOut receipts and allow for more ETH or tokens to be withdrawn than should be. We might want a setup that keeps a running log of all asset balances that have been approved for withdrawal and then trigger a pause if more are somehow withdrawn than expected. This would require some kind of persistent data store to work efficiently (like the current "alert" concept but with a timing window that spans from the start of the chain to now).


