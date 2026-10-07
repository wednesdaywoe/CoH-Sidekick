"""Parser for powercats.bin (power categories).

Format per record:
  u4 len
  string source
  string key
  --- category data (len - 8 bytes) ---
  string display
  string help
  string short_help
  string_array powersets
"""

from ._reader import open_parse7
from ._dataclasses import PowercatRecord


def parse_powercats(bin_path_or_data) -> list[PowercatRecord]:
    r = open_parse7(bin_path_or_data)

    # Data block: u4 size, then u4 count + records.
    # `_block_size` is unused BY NAME and load-bearing BY POSITION — read_u4 advances the
    # cursor four bytes. Removing the line misaligns every record that follows.
    _block_size = r.read_u4()
    count = r.read_u4()

    records = []
    for _ in range(count):
        rec_len = r.read_u4()
        sub = r.sub_reader(rec_len)

        source = sub.read_string()
        key = sub.read_string()
        display = sub.read_string()
        help_text = sub.read_string()
        short_help = sub.read_string()
        powersets = sub.read_string_array()

        records.append(PowercatRecord(
            source=source,
            key=key,
            display_name=display,
            help=help_text,
            short_help=short_help,
            powersets=powersets,
        ))

        r.skip(rec_len)

    return records
