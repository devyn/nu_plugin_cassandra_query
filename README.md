# nu_plugin_cassandra_query

This is a [Nushell](https://nushell.sh/) plugin called "cassandra_query".

I wrote this plugin mostly to make working with Cassandra for local development easier. Despite offering a SQL-like syntax, Cassandra queries are much more restrictive than SQL. While it is completely reasonable that these restrictions exist on production databases, they can make it difficult to explore data that exists on a development instance. The tools that exist to make running ad-hoc queries on Cassandra easier generally don't provide any options for doing additional operations on this data - for example, parsing embedded JSON. But all of this is very easy in Nushell.

## Installing

```nushell
> cargo install --path .
```

You need the Cassandra C++ driver installed. You may need to set `LIBRARY_PATH` if it can't be found automatically. On macOS with Homebrew, you will likely need `LIBRARY_PATH=/opt/homebrew/lib`

## Features

* CQL queries with data output to native Nushell types
* Paged, on-demand streaming queries (with customizable page size)
* Support for multiple contact points (localhost by default)
* Supported types:
  * All kinds of integers, floats, strings, booleans
  * Lists, maps, sets, tuples
  * Numeric (as string)
  * UUID (as string)
  * Bytes (native)
  * Duration (native)
  * DateTime (native), `date` and `time` (as strings)

### Planned features (not yet implemented)

* SSL support
* Query parameters with Nushell value input

## Usage

For details on supported options, see `help cassandra_query` from within Nushell.

```nushell
> plugin add ~/.cargo/bin/nu_plugin_cassandra_query
> plugin use cassandra_query
> cassandra-query "SELECT foo, bar FROM example.table"
╭───┬───────┬─────╮
│ # │  foo  │ bar │
├───┼───────┼─────┤
│ 0 │ hello │   1 │
│ 1 │ world │   2 │
╰───┴───────┴─────╯
> cassandra-query "SELECT id, json_column FROM example.json_table" | update json_column { from json }
╭───┬──────────────────────────────────────┬───────────────────────────────────────╮
│ # │                  id                  │              json_column              │
├───┼──────────────────────────────────────┼───────────────────────────────────────┤
│ 0 │ a32bfbb5-1303-4303-aca2-a4cb3122a8f9 │ ╭────────┬──────╮                     │
│   │                                      │ │ this   │ is   │                     │
│   │                                      │ │ a      │ json │                     │
│   │                                      │ │ column │ true │                     │
│   │                                      │ ╰────────┴──────╯                     │
│ 1 │ e35f0973-f297-4e65-ba2c-ee60f43492fc │ ╭────────┬──────────────────────────╮ │
│   │                                      │ │        │ ╭───┬──────┬───────────╮ │ │
│   │                                      │ │ nested │ │ # │ maps │ supported │ │ │
│   │                                      │ │        │ ├───┼──────┼───────────┤ │ │
│   │                                      │ │        │ │ 0 │ are  │    ❎     │ │ │
│   │                                      │ │        │ │ 1 │  ❎  │ too       │ │ │
│   │                                      │ │        │ ╰───┴──────┴───────────╯ │ │
│   │                                      │ ╰────────┴──────────────────────────╯ │
╰───┴──────────────────────────────────────┴───────────────────────────────────────╯
```
