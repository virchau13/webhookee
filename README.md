# webhookee

A simple webhook receiver.

## Usage
Execute `webhookee` and it will simply run until you terminate it. By default, `webhookee` will output its logs to standard output - this can be changed by specifying the `--log-file <file>` option.

## Configuration 

The configuration is read from `$XDG_CONFIG_HOME/webhookee/config.json` if `$XDG_CONFIG_HOME` is set,
otherwise defaulting to `~/.config/webhookee/config.json`. (The configuration file location can be overridden by the `--config` option.) The `config.json` file looks like this:
```json
{
    "port": 8000,
    "catchers": [{
        "methods": ["GET"],
        "path": "/deploy/myProject",
        "run": "~/.config/webhookee/deploy-myProject.sh --webhook",
        "validate": ["github", "$MYPROJECT_WEBHOOK_SECRET"]
    }, {
        "methods": ["GET", "POST"],
        "path": "/notify-on-webhook",
        "run": "notify-send \"$(cat /dev/stdin)\"",
        "validate": false
    }, {
        "methods": ["POST"],
        "path": "/deploy/mySecondProject",
        "run": "/usr/bin/mySecondProject --deploy-from-webhook",
        "validate": "/usr/bin/mySecondProject --validate-from-webhook"
    }]
}
```

### Detailed options

| Property | Description | Examples | 
| -------- | ----------- | ------- |
| `.port` | The port number, between 0 and 65535. | `8080` |
| `.catchers[i].path` | The URI path to trigger the webhook on. | `/any/path-like/t_h_i_s` |
| `.catchers[i].methods` | A list of [HTTP methods](https://developer.mozilla.org/en-US/docs/Web/HTTP/Methods) to trigger this webhook on. | `["GET", "POST"]` |
| `.catchers[i].validate` | How `webhookee` should validate that this webhook is coming from a trusted source (so that nobody can just arbitrarily trigger your webhooks.) It will only run `.catchers[i].run` if the request is validated. It can be any of the following: <ul> <li>The value `false`, which will result in no validation (not recommended).</li> <li>The value <code>["github", <i>KEY</i>]</code>, which will result in [validating via a GitHub signature](https://docs.github.com/en/developers/webhooks-and-events/webhooks/securing-your-webhooks) (used for webhooks on GitHub repositories). _`KEY`_ can either be the secret key itself (not recommended) or (if it starts with `$`) an environment variable that resolves to the secret key.</li> <li>Any string, in which case it will execute <code>/bin/sh -c <i>string</i></code>, pass the [request payload](#request-payload) to standard input, and if the exit code of the script is 0 the request will be validated. </li> </ul> | In order: <ul><li>`false`</li><li>`["github", "$WEBHOOK_SECRET"]`</li><li>`gpg --verify ~/my.sig -`</li> |
| `.catchers[i].run` | The script that will be run to handle the request (executed by <code>/bin/sh -c <i>run</i></code>). The [request payload](#request-payload) will be passed to standard input, and the response body will be the standard output of the process. | `cd ~/project; docker-compose restart` |

## Request payload
The following payload format will be used to describe the request:
```json
{
    "method": "POST",
    "path": "/apath",
    "headers": {
        "host": "my-site.com:30017",
        "user-agent": "curl/7.77.0",
        "accept": "*/*"
    },
    "body": "Anything could be here."
}
```

### Detailed options
| Property | Description | Examples | 
| -------- | ----------- | ------- |
| `.method` | The [HTTP method](https://developer.mozilla.org/en-US/docs/Web/HTTP/Methods). | `"PATCH"` |
| `.path` | The HTTP path that the request accessed. | `/webhook/trigger-this` |
| `.headers` | A key-value object of all the HTTP headers of the request. The header names will always be lowercased. The header value will be a string if it is valid UTF-8, otherwise it will be a byte array. | `{ "accept": "application/json", "x-proj-data": [108, 111, 108] }` |
| `.body` | The body of the request, empty if no body was present. It will be a string if the body is valid UTF-8, else it will be a byte array. | `"trigger your webhook"` |
