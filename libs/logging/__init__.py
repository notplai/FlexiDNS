import enum
import sys
import logging
from logging.handlers import RotatingFileHandler
import os
import traceback
from configparser import ConfigParser
from typing import Any, Union
import random

exception_words = [
    "Don't be sad!",
    "Sorry, but it's an exception.",
    "Oops, that didn't go as planned.",
    "We hit a tiny speed bump.",
    "Hang tight—I'll fix that.",
    "Well, that escalated quickly.",
    "Plot twist! That's an error.",
    "Glitch gremlins at work.",
    "Whoops! My bad.",
    "Abort mission… for now.",
    "Close, but not compile.",
    "That was unexpected.",
    "We zigged when we should've zagged.",
    "Not today, exception!",
    "That wasn't in the script.",
    "Let's try that again.",
    "Computers, right?",
    "It's not you, it's me.",
    "Even heroes throw exceptions.",
    "Out of cheese error. Redo from start.",
    "We tripped over a semicolon.",
    "404: Success not found.",
    "The hamsters fell off the wheel.",
    "Rain check on that operation.",
    "Time for a quick bug hunt.",
    "Take a breath; we got this.",
    "Tiny setback. Mighty comeback.",
    "Retry? Y/N",
    "If at first you don't succeed, iterate.",
    "You found a rare edge case!",
    "Achievement unlocked: Exception handled.",
    "That input and I need couples therapy.",
    "Stack traces are just treasure maps.",
    "On the bright side, logging works!",
    "I felt that exception in my soul.",
    "Let's debug like it's hot.",
    "The code gremlins are feisty today.",
    "Coffee required. Sending help.",
    "Beep boop… nope.",
    "Well, that didn't compile emotionally.",
    "Keep calm and catch exceptions.",
    "Oops! Let's roll back a sec.",
    "Unexpected end of optimism.",
    "Brace yourself—logs are coming.",
    "That variable ghosted us.",
    "Null pointer met its soulmate: nothing.",
    "Syntax? Never met her.",
    "We're not lost, just exploring.",
    "Edge case did parkour.",
    "That's a feature in disguise.",
    "Refactor dance break!",
    "One step back; two commits forward.",
    "Semicolons are overrated anyway.",
    "Exception hugged the code too hard.",
    "Mission failed successfully.",
    "Wheel of fortune says: try again.",
    "Today we learn; tomorrow we ship.",
    "Smol oops, big comeback.",
    "Rainy log, sunny fix.",
    "Error today, legend tomorrow.",
    "Gentle nudge: something went off.",
    "Tiny chaos detected. All good.",
    "Heads up—we'll retry.",
    "We'll laugh about this in prod. Maybe."
]

def parse(val: str) -> Union[bool, int, float, str]:
    val = val.strip()
    val_lower = val.lower()
    if val_lower == 'true':
        return True
    if val_lower == 'false':
        return False
    try:
        return float(val) if '.' in val else int(val)
    except ValueError:
        return val

def setup_handler(handler, level_includes, level_map):
    min_level = logging.CRITICAL
    for level_name, level in level_map.items():
        if level_name in level_includes and level < min_level:
            min_level = level
    handler.setLevel(min_level)
    return handler

def init():
    config = ConfigParser(allow_no_value=False, default_section='General')
    config.read('config.ini')
    
    enabled = config.getboolean('Logging', 'enabledFile', fallback=True)
    log_include = config.get('Logging', 'logIncluded').strip('",[] ').replace(' ', '').split(',')
    console_include = config.get('Logging', 'consoleIncluded').strip('",[] ').replace(' ', '').split(',')

    logger = logging.getLogger("UDIP")
    logger.setLevel(logging.DEBUG)

    log_format = logging.Formatter(
        "[%(asctime)s] | %(name)s %(levelname)s::%(api)s: %(message)s",
        datefmt='%Y-%m-%d %H:%M:%S'
    )

    level_map = {
        'debug': logging.DEBUG,
        'normal': logging.INFO,
        'error': logging.ERROR
    }

    # Console handler
    console_handler = logging.StreamHandler()
    console_handler.setFormatter(log_format)
    console_handler = setup_handler(console_handler, console_include, level_map)
    logger.addHandler(console_handler)

    # File handler
    if enabled:
        log_directory = 'logs'
        os.makedirs(log_directory, exist_ok=True)

        if config.getboolean('Logging', 'splitLog', fallback=True):
            for level_name, level in level_map.items():
                if level_name in log_include:
                    handler = RotatingFileHandler(
                        os.path.join(
                            log_directory,
                            f'udip.{level_name}.log' if level_name != 'normal' else 'udip.log'
                        ),
                        maxBytes=5 * 1024 * 1024,
                        backupCount=5
                    )
                    handler.setLevel(level)
                    handler.setFormatter(log_format)
                    logger.addHandler(handler)
        else:
            base_handler = RotatingFileHandler(
                os.path.join(log_directory, 'udip.log'),
                maxBytes=5 * 1024 * 1024,
                backupCount=5
            )
            base_handler.setFormatter(log_format)
            base_handler = setup_handler(base_handler, log_include, level_map)
            logger.addHandler(base_handler)

    # Global uncaught exception handler
    def handle_uncaught(exc_type, exc_value, exc_traceback):
        if issubclass(exc_type, KeyboardInterrupt):
            sys.__excepthook__(exc_type, exc_value, exc_traceback)
            return

        # Get the last traceback frame (where the exception occurred)
        __tb = traceback.extract_tb(exc_traceback)[-1]
        __package = __tb.filename
        __ln = __tb.lineno
        __func = __tb.name
        __name = __package.split("/")[-1]

        logger.error(
            f"Uncaught exception ------- {random.choice(exception_words)}\n-> At {__package} {__func}:{__ln}",
            exc_info=(exc_type, exc_value, exc_traceback),
            extra={"api": __name}
        )
    sys.excepthook = handle_uncaught

    return logger

logger = init()

class Logger:
    def __init__(self, integrate: str):
        self.integrate = integrate
    
    def log(self, message: Any, level: int = logging.INFO) -> None:
        logger.log(level=level, msg=message, extra={"api": self.integrate})

    def verbose(self, message: Any, level: int = logging.DEBUG) -> None:
        self.log(message, level)

    def exception(self, e: Exception) -> None:
        self.log(f"ERROR::{self.integrate}: {str(e)}\n{traceback.format_exc()}", logging.ERROR)
