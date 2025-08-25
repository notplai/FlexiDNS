import configparser
from datetime import date, datetime, timedelta
import math
import os
import sys
import asyncio
import re
from typing import NoReturn, Optional, Self, Union, Any

from libs.logging import Logger
from libs.converter import unixConvert

from libs.api.FetchAPI import icanhazip, ipify, ifconfig
from libs.api.cloudflare import CloudFlare
from libs.api.noip import NoIP
from libs.api.dyndns import DynDNS

import json
from ipaddress import ip_address
import argparse

logger = Logger("UDIP")

def parse(val) -> Union[bool, int, float, str, Any]:
    val = val.strip()
    if val.lower() == 'true':
        return True
    if val.lower() == 'false':
        return False
    try:
        if '.' in val:
            return float(val)
        return int(val)
    except ValueError:
        return val

# Load configuration
config = configparser.ConfigParser(allow_no_value=True, default_section='General')
config.read('config.ini')
queryAPI = config.get('General', 'queryAPI', fallback='ipify').strip('",')

LearningBehaviors = {
    option: [parse(v) for v in config.get('LearningBehavior', option, fallback='').strip('",[] ').replace(' ', '').split(',')]
    for option in config.options('LearningBehavior')
    if option != 'enabled' and config.get('LearningBehavior', option, fallback='').strip('",[] ').startswith('True')
}

def initialize_api() -> tuple[dict[str, Any], dict[str, dict[str, list[str]]]]:
    apis: dict[str, Any] = {}
    object_fqdn: dict[str, dict[str, list[str]]] = {}

    section = [
        _ for _ in config.sections()
        if _ and config.getboolean(_, "enabled", fallback=False)
    ]

    for _ in section:
        credentials = {
            "email": config.get(_, "email", fallback="").strip('",'),
            "password": config.get(_, "password", fallback="").strip('",')
        }
        cache = {
            "cache_timeout": config.getint(_, "cacheTimeout", fallback=172800),
            "cache_persistent": config.getboolean(_, "cachePersistent", fallback=False)
        }

        object_fqdn[_] = json.loads(config.get(_, "FQDN").strip("',"))
        apis[_] = globals()[_](**credentials, **cache)

    return apis, object_fqdn

# Initialize APIs
APIs, ObjectFQDNs = initialize_api()

async def call(instance: Any, fqdns: list[str], content: str) -> None:
    """Update DNS records for a given API."""
    sync_logger = Logger(instance.__class__.__name__)
    if not instance: return
    
    for fqdn in fqdns:
        sync_logger.log(f"Updating record: {fqdn} -> {content}")
        if ip_address(content).version.__eq__(4):
            instance.A(fqdn, content)
        elif ip_address(content).version.__eq__(6):
            instance.AAAA(fqdn, content)

class AsynchronousPeriodic:
    """Asynchronous loop for periodic tasks."""
    integrate = 'AsynchronousPeriodic'
    def __init__(self: Self) -> None:
        self.sync_logger = Logger(self.integrate)
        self.__hl = False

    async def sync(self: Self) -> int:
        """Run the asynchronous loop with exponential backoff on error."""
        total_seconds: int = 0
        timeshift = LearningBehaviors.get('timeshift')
        
        while True:
            try:
                # Retrieve public IP addresses using the configured API
                if queryAPI == 'ipify':
                    ipify_response = ipify(64).get_address(format='text')
                    if isinstance(ipify_response, dict):
                        inet_address = [ipify_response.get('ipv4', ''), ipify_response.get('ipv6', '')]
                    elif isinstance(ipify_response, str):
                        inet_address = [ipify_response]
                    else:
                        raise ValueError("Unexpected response type from ipify API")  
                elif queryAPI == 'icanhazip':
                    inet_address = [icanhazip().get().replace('\n', '').strip()]
                elif queryAPI.split('?')[0] == 'ifconfig':

                    ifinfo = ifconfig(queryAPI.split('?')[-1] if '?' in queryAPI else '/').get()
                    if isinstance(ifinfo, dict):
                        ip_addr = ifinfo.get('ip_addr', None)
                    else:
                        ip_addr = ifinfo
                    # Support both single IP and comma-separated list
                    if ip_addr is not None:
                        inet_address = [ip.strip() for ip in ip_addr.split(',')] if ',' in ip_addr else [ip_addr]
                    else:
                        inet_address = []
                
                else:
                    raise ValueError(f"Unsupported queryAPI: {queryAPI}. Supported APIs are 'ipify', 'icanzip' and 'ifconfig'.")
                
                inet_address_object: dict[str, Any] = {
                    "Iv4": inet_address[0] if inet_address and ip_address(inet_address[0]).version == 4 else None,
                    "Iv6": inet_address[1] if len(inet_address) > 1 and ip_address(inet_address[1]).version == 6 else None
                }
                
                if not (inet_address_object["Iv4"] or inet_address_object["Iv6"]):
                    self.sync_logger.log("Public IPv4 and IPv6 not retrieved, skipping update", 30)
                    return total_seconds or 0
                
                os.makedirs('.dumps', exist_ok=True)
                try:
                    with open('.dumps/found_address', 'r') as f:
                        found_address: dict[str, Any] = json.load(f)
                        if found_address == inet_address_object:
                            if not self.__hl:
                                self.sync_logger.log("Public address is same as before, skipping update")
                                self.__hl = True
                            return total_seconds or 0
                        f.close()
                        raise TimeoutError("Public address timed out")
                except Exception as e:
                    self.__hl = False
                    if isinstance(e, (FileNotFoundError, TimeoutError)):
                        with open('.dumps/found_address', 'w') as f:
                            json.dump(inet_address_object, f, indent=4)
                            f.close()
                    else: raise e

                tasks = []
                for object_name in APIs:
                    record_types = {
                        'A': ('A', 'Iv4'),
                        'AAAA': ('AAAA', 'Iv6')
                    }
                    
                    for dns_type, (record, ip_ver) in record_types.items():
                        if dns_type in ObjectFQDNs[object_name] and inet_address_object[ip_ver]:
                            tasks.append(call(
                                APIs[object_name],
                                ObjectFQDNs[object_name][record],
                                inet_address_object[ip_ver]
                            ))      
                if tasks:
                    await asyncio.gather(*tasks)
                return total_seconds

            except Exception as e:
                self.sync_logger.exception(e)
                if timeshift:
                    total_seconds += int(timeshift[1])
                    self.sync_logger.log(f"Retrying in {total_seconds} seconds.")
                    await asyncio.sleep(total_seconds)
                    continue
                else:
                    return total_seconds
        
    async def interval(self: Self, sync_time: Union[float, int]) -> NoReturn:
        self.sync_logger.log(f"Starting periodic DNS updates every {sync_time} seconds ({sync_time//60}m).")
        
        while True:
            start_time = asyncio.get_event_loop().time()
            await self.sync()
            elapsed_time = asyncio.get_event_loop().time() - start_time
            delta_time = max(0, sync_time - elapsed_time)

            self.sync_logger.log(f"Cycle completed in {elapsed_time:.2f}s. Sleeping for {delta_time:.2f}s.")
            await asyncio.sleep(delta_time)

    async def unix(self: Self, unix_time: float | int) -> NoReturn:
        unixl = unixConvert(unix_time)
        self.sync_logger.log(f"Starting periodic DNS updates at {unixl[2]:02d}:{unixl[1]:02d}:{unixl[0]:02d}. each 24 hours.")
        
        while True:
            now = datetime.now()
            target_time = now.replace(hour=unixl[2], minute=unixl[1], second=unixl[0], microsecond=0)
            
            if now >= target_time: target_time += timedelta(days=1)
            time_to_wait = (target_time - now).total_seconds()
            self.sync_logger.log(f"Waiting {time_to_wait} seconds until {target_time.ctime()}...")
            await asyncio.sleep(time_to_wait)
            target_time += timedelta(seconds=await self.sync())
            self.sync_logger.log("Cycle completed. Sleeping until next scheduled time.")

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description="UDIP Dynamic Updater", exit_on_error=True)
    parser.add_argument("-m", "--mode", type=str, choices=["unix", "interval", "prefer"], help="The mode of operation for the updater. 'unix' for Unix epoch time, 'interval' for periodic updates, 'prefer' for one-time sync.")
    parser.add_argument("-t", "--synctime", type=int, help="The sync time specifies the time between each loop check and update.")
    args = parser.parse_args()
    
    # Conditional validation
    if args.mode in ["unix", "interval"] and args.synctime is None:
        parser.error(f"--synctime (-t) is required when --mode (-m) is '{args.mode}'")
    if args.mode == 'prefer' and args.synctime is not None:
        parser.error("--synctime (-t) is not allowed when --mode (-m) is 'prefer'")

    mode = str(args.mode) if args.mode else config.get("General", "mode", fallback="intervalTime").strip('",')
    
    syncTime = math.nan if args.mode in ['prefer'] else int(args.synctime) if args.synctime else config.getint("General", "syncTime", fallback=36000)

    try:    
        logger.log(f">>====<< {re.sub(r'(?<!^)(?=[A-Z])', ' ', mode).title()} execute >>====<<")
        if mode in ["intervalTime", "interval"]:
            asyncio.run(AsynchronousPeriodic().interval(syncTime))
        elif mode in ["unixEpoch", "unix"]:
            rtime = unixConvert(syncTime)
            asyncio.run(AsynchronousPeriodic().unix(syncTime))
        elif args.mode in ['prefer']:
            asyncio.run(AsynchronousPeriodic().sync())
            logger.log("Preferred one-time sync completed.")

        else: raise ValueError("mode must be either 'intervalTime' or 'unixEpoch'.")

        logger.log(">>====<< Execution completed >>====<<", level=10)
    except KeyboardInterrupt as e:
        logger.log("KeyboardInterrupt detected. Exiting...", level=10)
        sys.exit(0)
