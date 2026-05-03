from utils import compute

def login(user: str, pwd: str) -> bool:
    return compute(pwd) == user

def _internal():
    pass

class Auth:
    def __init__(self): pass

def some_decorator(f):
    import functools
    @functools.wraps(f)
    def wrapper(*a, **kw):
        return f(*a, **kw)
    return wrapper

@some_decorator
def public_decorated():
    pass

@some_decorator
@some_decorator
def stacked_decorators():
    pass

@some_decorator
class DecoratedCls:
    pass

@some_decorator
def _private_decorated():
    pass
