#import <Foundation/Foundation.h>

@interface Foo : NSObject
@property (nonatomic, strong) NSString *name;
- (void)doThing:(NSString *)arg;
+ (instancetype)create;
@end

@implementation Foo
- (void)doThing:(NSString *)arg {
    NSLog(@"%@", arg);
}
+ (instancetype)create {
    return [[Foo alloc] init];
}
@end

@protocol Drawable
- (void)draw;
@end
