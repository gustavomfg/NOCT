# Provider access and subscriptions

[Português do Brasil](provider-subscription-access.pt-BR.md)

Nocturne Studio connects to AI through API keys and local runtimes. It does not
offer a login with a consumer chat subscription.

- **API key:** works with any configured provider; usage is billed by that
  provider and is separate from any chat plan.
- **Local runtime** (Ollama, LM Studio): no key and no vendor account; the model
  runs on your machine.
- **Aggregators** (for example OpenRouter): one key gives access to models from
  several vendors, billed through the aggregator.

Consumer subscriptions do not expose a supported, general-purpose login for
third-party applications, so Nocturne does not ask for, store or reuse one. See
[Providers](providers.md) for the connection options.
